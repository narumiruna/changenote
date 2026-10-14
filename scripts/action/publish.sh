#!/usr/bin/env bash
set -euo pipefail
# shellcheck source=scripts/action/publication.sh
source "${BASH_SOURCE[0]%/*}/publication.sh"
setup
output published false
output published-packages '[]'
output release-commit ''
verify_publication_batch
# The repository workflow passes the preview selection. Composite callers omit it.
if [[ $# -gt 0 && $release != "$1" ]]; then
  fail 'Release batch changed after preview; use an explicit retry.'
fi
[[ -n $release ]] || exit 0
output release-commit "$release"
# Inspect all remote objects before creating anything. API failure is not absence.
inspect_publication
require_publication_ready
"$cli" tag
changed='[]'
index=0
while IFS= read -r item; do
  index=$((index + 1))
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
    if gh release create "$tag" --repo "$GITHUB_REPOSITORY" --verify-tag --title "$tag" --notes-file "$temp/notes/$index.md"; then
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
