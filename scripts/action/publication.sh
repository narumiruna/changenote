#!/usr/bin/env bash
# Shared batch verification and remote inspection; callers own setup and writes.
# shellcheck source=scripts/action/common.sh
source "${BASH_SOURCE[0]%/*}/common.sh"

verify_publication_batch() {
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
  [[ -n $release ]] || return 0
  git show "$release:.changenote/releases.json" > "$temp/batch.json" || fail 'Release batch was removed; inspect history manually.'
  jq -e 'type == "array" and length > 0' "$temp/batch.json" >/dev/null || fail 'Invalid or empty release batch.'
  parent=$(git rev-parse "$release^")
  checkout_work "$parent"
  # Reconstruct the version commit from its parent's notes. This validates all fields,
  # bumps, deleted notes, versions, changelogs, paths, and the batch without a new schema.
  "$cli" status > "$temp/expected.json"
  jq -e --slurpfile expected "$temp/expected.json" '. == ($expected[0] | map(del(.notes)))' "$temp/batch.json" >/dev/null || fail 'Batch does not match the parent release plan.'
  local generated=''
  if "$cli" version > "$temp/applied.json"; then
    stage_release "$temp/batch.json"
    generated=$(git write-tree)
  fi
  if [[ $generated != "$(git rev-parse "$release^{tree}")" ]]; then
    # Earlier releases did not refresh lockfiles. Accept only a complete profile.
    git reset --hard "$parent"
    "$cli" version --no-lockfiles > "$temp/applied.json"
    stage_release "$temp/batch.json"
    [[ $(git write-tree) == "$(git rev-parse "$release^{tree}")" ]] || fail 'Release commit contains changes beyond the generated release tree.'
  fi
  git reset --hard "$release"
  # Validate every body before any remote writes, including later batch members.
  local item package version index=0
  mkdir -p "$temp/notes"
  while IFS= read -r item; do
    index=$((index + 1))
    package=$(jq -r '.package' <<< "$item")
    version=$(jq -r '.version' <<< "$item")
    "$cli" release-notes --package="$package" --version "$version" > "$temp/notes/$index.md"
  done < <(jq -c '.[]' "$temp/batch.json")
}

inspect_publication() {
  gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" > "$temp/releases.json"
  jq -e 'type == "array" and length > 0 and all(.[]; type == "array" and all(.[]; (.tag_name | type == "string" and length > 0) and (.draft | type == "boolean")))' "$temp/releases.json" >/dev/null || fail 'Invalid GitHub release response.'
  printf '[]\n' > "$temp/publication-plan.json"
  local item tag action conflicts has_release draft
  while IFS= read -r item; do
    tag=$(jq -r '.tag' <<< "$item")
    remote_tag "$tag"
    has_release=$(jq --arg tag "$tag" 'any(.[][]; .tag_name == $tag)' "$temp/releases.json")
    draft=$(jq --arg tag "$tag" 'any(.[][]; .tag_name == $tag and .draft)' "$temp/releases.json")
    conflicts='[]'
    if [[ -n $tag_sha && $tag_sha != "$release" ]]; then
      conflicts=$(jq -c --arg reason "Remote tag points to another commit: $tag" '. + [$reason]' <<< "$conflicts")
    fi
    if [[ $draft == true ]]; then
      conflicts=$(jq -c --arg reason "Existing release is a draft: $tag" '. + [$reason]' <<< "$conflicts")
    fi
    if [[ $has_release == true && -z $tag_sha ]]; then
      conflicts=$(jq -c --arg reason "Existing release has no remote tag: $tag" '. + [$reason]' <<< "$conflicts")
    fi
    if [[ $conflicts != '[]' ]]; then action=conflict
    elif [[ -z $tag_sha ]]; then action=create-tag-and-release
    elif [[ $has_release == false ]]; then action=create-release
    else action=none
    fi
    jq --argjson item "$item" --arg action "$action" --argjson conflicts "$conflicts" \
      '. + [{name: $item.package, version: $item.version, tag: $item.tag, action: $action, conflicts: $conflicts}]' \
      "$temp/publication-plan.json" > "$temp/next-plan.json"
    cp -- "$temp/next-plan.json" "$temp/publication-plan.json"
  done < <(jq -c '.[]' "$temp/batch.json")
  ready=$(jq 'all(.[]; .action != "conflict")' "$temp/publication-plan.json")
}

require_publication_ready() {
  if [[ $ready != true ]]; then
    jq -r '.[].conflicts[]' "$temp/publication-plan.json" >&2
    fail 'Publication conflicts require manual inspection.'
  fi
}
