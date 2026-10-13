#!/usr/bin/env bash
# Shared setup. Source this file from an Action entry point.
# These globals are used by the entry points.
# shellcheck disable=SC2034
set -euo pipefail

fail() { printf '%s\n' "$*" >&2; exit 1; }
output() { printf '%s=%s\n' "$1" "$2" >> "$GITHUB_OUTPUT"; }

# Use the API's HTTPS URL, not origin's possibly different push URL or SSH credentials.
remote_git() {
  git -c credential.helper= -c http.extraHeader= \
    -c "http.${remote_url}.extraHeader=" \
    -c "http.${remote_url}.extraHeader=AUTHORIZATION: basic ${auth}" "$@"
}

cleanup() {
  if [[ -n ${work:-} ]]; then git -C "$root" worktree remove --force "$work" >/dev/null 2>&1 || true; fi
  rm -rf -- "$temp"
}

setup() {
  [[ $(uname -s) == Linux ]] || fail 'Only Linux runners are supported.'
  for tool in git gh jq changenote mktemp cp base64; do
    command -v "$tool" >/dev/null || fail "Missing prerequisite: $tool"
  done
  : "${GH_TOKEN:?github-token is required}" "${GITHUB_REPOSITORY:?repository is required}" "${GITHUB_OUTPUT:?output file is required}"
  [[ $GITHUB_REPOSITORY =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail 'Invalid repository.'
  root=$(git rev-parse --show-toplevel)
  cd "$root"
  [[ -z $(git status --porcelain) ]] || fail 'A clean checkout is required.'
  [[ -z ${GIT_INDEX_FILE:-} && -z ${GIT_WORK_TREE:-} && -z ${GIT_DIR:-} ]] || fail 'Custom Git worktree variables are not supported.'
  temp=$(mktemp -d)
  work=''
  trap cleanup EXIT
  # Copy before checking out older revisions. Consumer builds may live in target/.
  cp -- "$(command -v changenote)" "$temp/changenote"
  cli="$temp/changenote"
  gh api "repos/$GITHUB_REPOSITORY" > "$temp/repo.json"
  jq -e '.permissions.push == true and (.clone_url | type == "string")' "$temp/repo.json" >/dev/null || fail 'Repository write permission is required.'
  remote_url=$(jq -r '.clone_url' "$temp/repo.json")
  base=${INPUT_BASE_BRANCH:-$(jq -r '.default_branch' "$temp/repo.json")}
  git check-ref-format "refs/heads/$base" || fail 'Invalid base branch.'
  auth=$(printf 'x-access-token:%s' "$GH_TOKEN" | base64 -w0)
  local fetch_args=(--no-tags)
  if [[ $(git rev-parse --is-shallow-repository) == true ]]; then fetch_args+=(--unshallow); fi
  remote_git fetch "${fetch_args[@]}" "$remote_url" "+refs/heads/$base:refs/remotes/origin/$base"
  base_sha=$(git rev-parse "refs/remotes/origin/$base")
  export GIT_AUTHOR_NAME='github-actions[bot]' GIT_COMMITTER_NAME='github-actions[bot]'
  export GIT_AUTHOR_EMAIL='41898282+github-actions[bot]@users.noreply.github.com'
  export GIT_COMMITTER_EMAIL="$GIT_AUTHOR_EMAIL"
}

checkout_work() {
  work="$temp/work"
  git worktree add --detach "$work" "$1"
  cd "$work"
  [[ ! -L .changenote/releases.json ]] || fail 'Action output paths cannot be symlinks: .changenote/releases.json'
}

stage_release() {
  local plan=$1 path
  local paths=(.changenote/releases.json)
  while IFS= read -r -d '' path; do
    paths+=("$path")
  done < <(jq -j --slurpfile config .changenote/config.json \
    '.[] | $config[0].packages[.package] | (.path + "/" + .version_file + "\u0000"), (.path + "/CHANGELOG.md\u0000")' "$plan")
  while IFS= read -r -d '' path; do
    if [[ $path == .changenote/*.md && ${path#.changenote/} != */* ]]; then paths+=("$path"); fi
  done < <(git diff --diff-filter=D --name-only -z)
  for path in "${paths[@]}"; do
    [[ ! -L $path ]] || fail "Action output paths cannot be symlinks: $path"
  done
  git --literal-pathspecs add -- "${paths[@]}"
}

# Read the remote's peeled tag identity. Never overwrite a local tag to hide a conflict.
remote_tag() {
  local tag=$1 listing
  listing=$(remote_git ls-remote --tags "$remote_url" "refs/tags/$tag" "refs/tags/$tag^{}")
  tag_sha=''
  if [[ -n $listing ]]; then
    tag_sha=$(awk '$2 ~ /\^\{\}$/ {print $1}' <<< "$listing")
    if [[ -z $tag_sha ]]; then tag_sha=$(awk 'NR == 1 {print $1}' <<< "$listing"); fi
  fi
}
