#!/usr/bin/env bash
# Prepare offline lockfile tools from validated release inputs in temporary trees.
set -euo pipefail
# shellcheck source=scripts/action/publication.sh
source "${BASH_SOURCE[0]%/*}/publication.sh"
export RUSTUP_TOOLCHAIN=${INPUT_RUST_TOOLCHAIN:-stable}
mode=${1:?Expected discover or warm}
[[ $mode == discover || $mode == warm ]] || fail 'Unknown environment operation.'
setup_readonly
[[ ${GITHUB_REF:-} == "refs/heads/$base" ]] || fail 'Run the release Action on its selected base branch.'
operation=${INPUT_OPERATION:-auto}
declare -A tools=() seen=()

inspect_environment() {
  local revision=$1 path directory lock tool component identity
  git -c core.hooksPath=/dev/null checkout --quiet --detach "$revision"
  "$cli" status > "$temp/environment-plan.json"
  while IFS= read -r -d '' path; do
    case ${path##*/} in
      Cargo.toml) lock=Cargo.lock; tool=cargo ;;
      pyproject.toml) lock=uv.lock; tool=uv ;;
      package.json) lock='package-lock.json'; tool=npm ;;
      *) continue ;;
    esac
    directory=$(dirname -- "$path")
    while :; do
      if [[ -e $directory/$lock || -L $directory/$lock ]]; then
        component="$directory/$lock"
        while [[ $component != . ]]; do
          [[ ! -L $component ]] || fail "Lockfile paths cannot contain symlinks: $directory/$lock"
          component=$(dirname -- "$component")
        done
        [[ -f $directory/$lock && -f $directory/${path##*/} && ! -L $directory/${path##*/} ]] || fail "Lockfile and owning manifest must be regular files: $directory/$lock"
        [[ $tool != npm || ! -e $directory/npm-shrinkwrap.json ]] || fail 'npm-shrinkwrap.json is not supported.'
        tools[$tool]=true
        identity="$revision:$directory:$tool"
        if [[ $mode == warm && -z ${seen[$identity]+present} ]]; then
          seen[$identity]=1
          (
            cd -- "$directory"
            # Package fetching must not receive the Action's GitHub write token.
            unset GH_TOKEN GITHUB_TOKEN
            case $tool in
              cargo) cargo fetch ;;
              npm) npm install --ignore-scripts --no-audit --no-fund ;;
              uv) UV_PROJECT_ENVIRONMENT="$temp/uv-environment" uv sync --no-install-project --no-install-workspace --no-build ;;
            esac
          )
        fi
        break
      fi
      [[ $directory != . ]] || break
      directory=$(dirname -- "$directory")
    done
  done < <(jq -j --slurpfile config .changenote/config.json '
    [.[] | $config[0].packages[.package] | .path as $directory |
      (.version_file, (.version_outputs // [])[]) |
      ($directory + "/" + . | split("/") | map(select(. != "." and . != "")) | join("/"))]
    | unique[] + "\u0000"' "$temp/environment-plan.json")
}

if [[ $operation != manage-pr ]]; then
  find_publication_batch
  if [[ $operation == publish ]]; then
    [[ ${INPUT_EXPECTED_RELEASE_COMMIT:-} == "$release" ]] || fail 'Release batch changed after selection; select and validate it again.'
  fi
  if [[ -n $release ]]; then inspect_environment "$parent"; fi
fi
pr_environment_ready=true
if [[ $operation == auto ]]; then
  # A fresh process preserves errexit and isolates current-plan failures.
  if INPUT_OPERATION=manage-pr GITHUB_OUTPUT="$temp/pr-environment.outputs" bash "${BASH_SOURCE[0]}" "$mode"; then
    if [[ $mode == discover ]]; then
      while IFS='=' read -r key value; do
        case $key in cargo|npm|uv) if [[ $value == true ]]; then tools[$key]=true; fi ;; esac
      done < "$temp/pr-environment.outputs"
    fi
  else
    pr_environment_ready=false
    printf 'Current PR environment preparation failed; historical publication remains available.\n' >&2
  fi
elif [[ $operation == manage-pr ]]; then
  inspect_environment "$base_sha"
fi
if [[ $mode == discover ]]; then
  for tool in cargo npm uv; do output "$tool" "${tools[$tool]:-false}"; done
fi
output pr-environment-ready "$pr_environment_ready"
