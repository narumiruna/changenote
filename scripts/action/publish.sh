#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/action/common.sh
source "${BASH_SOURCE[0]%/*}/common.sh"
setup
output published false
output published-packages '[]'
output release-commit ''
# Explicit first-parent diffs also select merge commits whose tree matches the PR head.
release=''
while IFS= read -r commit; do
  parent=$(git rev-parse "$commit^" 2>/dev/null || true)
  if [[ -z $parent || $parent == "$commit^" ]]; then
    if git cat-file -e "$commit:.changenote/releases.json" 2>/dev/null; then
      fail 'A release batch in a root commit has no verifiable parent.'
    fi
    break
  fi
  if ! git diff --quiet "$parent" "$commit" -- .changenote/releases.json; then
    release=$commit
    break
  fi
done < <(git rev-list --first-parent "$base_sha")
[[ -n $release ]] || exit 0
output release-commit "$release"
git show "$release:.changenote/releases.json" > "$temp/batch.json" || fail 'Release batch was removed; inspect history manually.'
jq -e 'type == "array" and length > 0' "$temp/batch.json" >/dev/null || fail 'Invalid or empty release batch.'
parent=$(git rev-parse "$release^")
checkout_work "$parent"
# Reconstruct the version commit from its parent's notes. This validates all fields,
# bumps, deleted notes, versions, changelogs, paths, and the batch without a new schema.
"$cli" status > "$temp/expected.json"
jq -e --slurpfile expected "$temp/expected.json" '. == $expected[0]' "$temp/batch.json" >/dev/null || fail 'Batch does not match the parent release plan.'
"$cli" version > "$temp/applied.json"
stage_release "$temp/batch.json"
[[ $(git write-tree) == "$(git rev-parse "$release^{tree}")" ]] || fail 'Release commit contains changes beyond the generated release tree.'
git reset --hard "$release"
# Inspect all remote objects before creating anything. API failure is not absence.
gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" > "$temp/releases.json"
while IFS= read -r tag; do
  remote_tag "$tag"
  [[ -z $tag_sha || $tag_sha == "$release" ]] || fail "Remote tag points to another commit: $tag"
  if jq -e --arg tag "$tag" 'any(.[][]; .tag_name == $tag and .draft == true)' "$temp/releases.json" >/dev/null; then
    fail "Existing release is a draft: $tag"
  fi
  if jq -e --arg tag "$tag" 'any(.[][]; .tag_name == $tag)' "$temp/releases.json" >/dev/null; then
    [[ -n $tag_sha ]] || fail "Existing release has no remote tag: $tag"
  fi
done < <(jq -r '.[].tag' "$temp/batch.json")
"$cli" tag
changed='[]'
while IFS= read -r item; do
  tag=$(jq -r '.tag' <<< "$item")
  did_publish=false
  remote_tag "$tag"
  [[ -z $tag_sha || $tag_sha == "$release" ]] || fail "Remote tag changed during publication: $tag"
  if [[ -z $tag_sha ]]; then
    if remote_git push "$remote_url" "refs/tags/$tag:refs/tags/$tag"; then
      did_publish=true
    else
      remote_tag "$tag"
      [[ $tag_sha == "$release" ]] || fail "Tag push failed: $tag"
    fi
  fi
  # Always recheck the remote identity, including after successful pushes.
  remote_tag "$tag"
  [[ $tag_sha == "$release" ]] || fail "Remote tag identity changed: $tag"
  if ! jq -e --arg tag "$tag" 'any(.[][]; .tag_name == $tag)' "$temp/releases.json" >/dev/null; then
    jq -r '.notes | join("\n\n")' <<< "$item" > "$temp/notes.md"
    if gh release create "$tag" --repo "$GITHUB_REPOSITORY" --verify-tag --title "$tag" --notes-file "$temp/notes.md"; then
      did_publish=true
    else
      # Only a verified existing release is an acceptable race result.
      gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" > "$temp/rechecked.json"
      jq -e --arg tag "$tag" 'any(.[][]; .tag_name == $tag and .draft == false)' "$temp/rechecked.json" >/dev/null || fail "Release creation failed: $tag"
      remote_tag "$tag"
      [[ $tag_sha == "$release" ]] || fail "Release tag conflict: $tag"
    fi
  fi
  remote_tag "$tag"
  [[ $tag_sha == "$release" ]] || fail "Remote tag changed while creating the release: $tag"
  if [[ $did_publish == true ]]; then
    changed=$(jq -c --argjson item "$item" '. + [{name: $item.package, version: $item.version}]' <<< "$changed")
    output published true
    output published-packages "$changed"
  fi
done < <(jq -c '.[]' "$temp/batch.json")
