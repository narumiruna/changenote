#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/action/publication.sh
source "${BASH_SOURCE[0]%/*}/publication.sh"
: "${GITHUB_OUTPUT:?output file is required}"
output ready false
output release-commit ''
output publication-plan '[]'
[[ $(uname -s) == Linux ]] || fail 'Only Linux runners are supported.'
for tool in git gh jq changenote mktemp cp base64; do
  command -v "$tool" >/dev/null || fail "Missing prerequisite: $tool"
done
: "${GH_TOKEN:?github-token is required}" "${GITHUB_REPOSITORY:?repository is required}" "${GITHUB_OUTPUT:?output file is required}"
[[ $GITHUB_REPOSITORY =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail 'Invalid repository.'
[[ -z ${GIT_INDEX_FILE:-} && -z ${GIT_WORK_TREE:-} && -z ${GIT_DIR:-} ]] || fail 'Custom Git worktree variables are not supported.'
temp=$(mktemp -d)
work=''
trap cleanup EXIT
cp -- "$(command -v changenote)" "$temp/changenote"
cli="$temp/changenote"
gh api "repos/$GITHUB_REPOSITORY" > "$temp/repo.json"
jq -e '.clone_url | type == "string" and length > 0' "$temp/repo.json" >/dev/null || fail 'Repository clone URL is required.'
remote_url=$(jq -r '.clone_url' "$temp/repo.json")
base=${INPUT_BASE_BRANCH:-$(jq -r '.default_branch' "$temp/repo.json")}
git check-ref-format "refs/heads/$base" || fail 'Invalid base branch.'
auth=$(printf 'x-access-token:%s' "$GH_TOKEN" | base64 -w0)
# Do not read or update any refs, index, files, or identity in the caller's checkout.
root="$temp/repository"
git -c init.templateDir= init --quiet "$root"
cd "$root"
remote_git fetch --no-tags "$remote_url" "refs/heads/$base"
base_sha=$(git rev-parse FETCH_HEAD)
verify_publication_batch
if [[ -z $release ]]; then
  output ready true
  printf 'No release batch to publish.\n'
  exit 0
fi
inspect_publication
output release-commit "$release"
output publication-plan "$(jq -c . "$temp/publication-plan.json")"
output ready "$ready"
printf 'Release commit: %s\n' "$release"
jq -r '.[] | "\(.name)@\(.version): \(.action)", (.conflicts[] | "  " + .)' "$temp/publication-plan.json"
require_publication_ready
