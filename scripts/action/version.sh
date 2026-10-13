#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/action/common.sh
source "${BASH_SOURCE[0]%/*}/common.sh"
setup
branch="narumi/chore/changenote-release-$base"
git check-ref-format "refs/heads/$branch" || fail 'Base branch cannot form a release branch.'
marker='<!-- changenote-release-pr -->'
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/pulls?state=all&per_page=100" > "$temp/pulls.json"
jq --arg branch "$branch" --arg base "$base" --arg repo "$GITHUB_REPOSITORY" --arg marker "$marker" \
  '[.[][] | select(.head.ref == $branch and .base.ref == $base and .head.repo.full_name == $repo) | select((.body // "") | startswith($marker))]' \
  "$temp/pulls.json" > "$temp/managed.json"
# A similarly named unmarked PR does not establish ownership.
unmanaged=$(jq --arg branch "$branch" --arg base "$base" --arg repo "$GITHUB_REPOSITORY" --arg marker "$marker" \
  '[.[][] | select(.state == "open" and .head.ref == $branch and .base.ref == $base and .head.repo.full_name == $repo) | select(((.body // "") | startswith($marker)) | not)] | length' "$temp/pulls.json")
[[ $unmanaged == 0 ]] || fail 'Release branch has an unmanaged open PR.'
jq '[.[] | select(.state == "open")]' "$temp/managed.json" > "$temp/open.json"
[[ $(jq length "$temp/open.json") -le 1 ]] || fail 'Multiple managed PRs exist.'
number=$(jq -r '.[0].number // empty' "$temp/open.json")
listing=$(remote_git ls-remote --heads "$remote_url" "refs/heads/$branch")
previous=$(awk '{print $1}' <<< "$listing")
if [[ -n $previous ]]; then
  [[ $(jq length "$temp/managed.json") -gt 0 ]] || fail 'Existing release branch has no managed PR history.'
  remote_git fetch --no-tags "$remote_url" "refs/heads/$branch"
fi
checkout_work "$base_sha"
"$cli" status > "$temp/plan.json"
output pr-number "${number:-}"
output pr-operation none
if [[ $(jq length "$temp/plan.json") == 0 ]]; then
  output has-notes false
  if [[ -n $number ]]; then
    printf '{"state":"closed"}\n' > "$temp/request.json"
    gh api --method PATCH "repos/$GITHUB_REPOSITORY/pulls/$number" --input "$temp/request.json" >/dev/null
    output pr-operation closed
  fi
  exit 0
fi
output has-notes true
"$cli" version > "$temp/applied.json"
jq -e --slurpfile expected "$temp/plan.json" '. == $expected[0]' "$temp/applied.json" >/dev/null || fail 'Version output differs from status.'
stage_release "$temp/plan.json"
tree=$(git write-tree)
expected_head=$previous
if [[ -z $previous || $(git rev-parse "$previous^{tree}") != "$tree" ]]; then
  git commit -m "${INPUT_COMMIT_MESSAGE:-chore(release): version packages}"
  remote_git push --force-with-lease="refs/heads/$branch:$previous" "$remote_url" "HEAD:refs/heads/$branch"
  expected_head=$(git rev-parse HEAD)
fi
listing=$(remote_git ls-remote --heads "$remote_url" "refs/heads/$branch")
[[ $(awk '{print $1}' <<< "$listing") == "$expected_head" ]] || fail 'Release branch moved after generation.'
jq -r --arg marker "$marker" \
  '$marker + "\n\nMerge this PR to release these versions. New notes update this PR.\n\n" + (map("## " + .package + ": " + .old_version + " → " + .version + " (" + .bump + ")\n\n" + (.notes | join("\n\n"))) | join("\n\n"))' \
  "$temp/plan.json" > "$temp/body.md"
if [[ $(wc -c < "$temp/body.md") -gt 60000 ]]; then
  printf '%s\n\nRelease details exceed the PR body limit. See the changed version files and changelogs.\n' "$marker" > "$temp/body.md"
fi
jq -n --arg title "${INPUT_PR_TITLE:-chore(release): version packages}" --rawfile body "$temp/body.md" \
  --arg base "$base" --arg head "$branch" '{title: $title, body: $body, base: $base, head: $head}' > "$temp/request.json"
if [[ -n $number ]]; then
  jq 'del(.head)' "$temp/request.json" > "$temp/update.json"
  gh api --method PATCH "repos/$GITHUB_REPOSITORY/pulls/$number" --input "$temp/update.json" >/dev/null
  output pr-operation updated
else
  if ! gh api --method POST "repos/$GITHUB_REPOSITORY/pulls" --input "$temp/request.json" > "$temp/created.json"; then
    # The API can succeed remotely but lose its response. Check before rollback.
    gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/pulls?state=open&per_page=100" > "$temp/rechecked.json"
    number=$(jq -r --arg branch "$branch" --arg base "$base" --arg repo "$GITHUB_REPOSITORY" --arg marker "$marker" \
      '[.[][] | select(.state == "open" and .head.ref == $branch and .base.ref == $base and .head.repo.full_name == $repo) | select((.body // "") | startswith($marker))][0].number // empty' "$temp/rechecked.json")
    if [[ -z $number ]]; then
      if [[ -z $previous ]]; then
        # Delete only the new branch from this run, and only if it has not moved.
        pushed=$(git rev-parse HEAD)
        remote_git push --force-with-lease="refs/heads/$branch:$pushed" "$remote_url" ":refs/heads/$branch"
      fi
      fail 'PR creation failed. Check pull requests write permission and repository settings.'
    fi
  else
    number=$(jq -er '.number' "$temp/created.json")
  fi
  output pr-number "$number"
  output pr-operation created
fi
