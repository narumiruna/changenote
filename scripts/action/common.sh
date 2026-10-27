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
  # Repository permissions do not describe installation token access.
  # Git pushes and API writes enforce the token's actual permissions.
  jq -e '.clone_url | type == "string"' "$temp/repo.json" >/dev/null || fail 'Repository clone URL is required.'
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
  local plan=$1 path directory lock component
  local paths=(.changenote/releases.json) unique=()
  local -A seen=()
  while IFS= read -r -d '' path; do
    paths+=("$path")
    case ${path##*/} in
      Cargo.toml) lock=Cargo.lock ;;
      pyproject.toml) lock=uv.lock ;;
      package.json) lock=package-lock.json ;;
      *) continue ;;
    esac
    directory=$(dirname -- "$path")
    while :; do
      if [[ -e $directory/$lock || -L $directory/$lock ]]; then
        paths+=("$directory/$lock")
        break
      fi
      [[ $directory != . ]] || break
      directory=$(dirname -- "$directory")
    done
  done < <(jq -j --slurpfile config .changenote/config.json \
    '[.[] | $config[0].packages[.package] | .path as $directory |
      (.version_file, (.version_outputs // [])[]) |
      ($directory + "/" + . | split("/") | map(select(. != "." and . != "")) | join("/"))]
      | unique[] + "\u0000"' "$plan")
  while IFS= read -r -d '' path; do
    paths+=("$path")
  done < <(jq -j --slurpfile config .changenote/config.json \
    '.[] | select($config[0].changelog.enabled != false) | $config[0].packages[.package] | .path + "/CHANGELOG.md\u0000"' "$plan")
  while IFS= read -r -d '' path; do
    if [[ $path == .changenote/*.md && ${path#.changenote/} != */* ]]; then paths+=("$path"); fi
  done < <(git diff --diff-filter=D --name-only -z)
  for path in "${paths[@]}"; do
    while [[ $path == ./* ]]; do path=${path#./}; done
    [[ -z ${seen[$path]+present} ]] || continue
    seen[$path]=1
    # Check ancestors too: Git must not stage outputs through directory symlinks.
    component=$path
    while [[ $component != . ]]; do
      [[ ! -L $component ]] || fail "Action output paths cannot be symlinks: $path"
      component=$(dirname -- "$component")
    done
    unique+=("$path")
  done
  git --literal-pathspecs add -- "${unique[@]}"
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
