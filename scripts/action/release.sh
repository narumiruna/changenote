#!/usr/bin/env bash
# Public high-level operations. Existing scripts own replay and remote writes.
set -euo pipefail
# shellcheck source=scripts/action/common.sh
source "${BASH_SOURCE[0]%/*}/common.sh"

validate_inputs() {
  [[ $(uname -s) == Linux ]] || fail 'Only Linux runners are supported.'
  case ${GITHUB_EVENT_NAME:-} in
    push|workflow_dispatch) ;;
    *) fail 'Use the release Action on push or workflow_dispatch, not contributor PR events.' ;;
  esac
  operation=${INPUT_OPERATION:-auto}
  case $operation in
    auto)
      if [[ $GITHUB_EVENT_NAME == workflow_dispatch ]]; then operation=manage-pr; fi
      ;;
    manage-pr|select|publish) ;;
    retry-publication)
      [[ $GITHUB_EVENT_NAME == workflow_dispatch ]] || fail 'retry-publication requires workflow_dispatch.'
      ;;
    *) fail 'operation must be auto, manage-pr, select, publish, or retry-publication.' ;;
  esac
  [[ ${INPUT_RETRY:-false} == false || ${INPUT_RETRY:-false} == true ]] || fail 'retry must be true or false.'
  if [[ ${INPUT_RETRY:-false} == true ]]; then
    [[ $operation == select && $GITHUB_EVENT_NAME == workflow_dispatch ]] || fail 'retry=true requires select on workflow_dispatch.'
  fi
  if [[ $operation == publish ]]; then
    [[ ${INPUT_EXPECTED_RELEASE_COMMIT:-} =~ ^[0-9a-f]{40}$ ]] || fail 'publish requires expected-release-commit with a full lowercase commit SHA.'
  else
    [[ -z ${INPUT_EXPECTED_RELEASE_COMMIT:-} ]] || fail 'expected-release-commit is only valid with publish.'
  fi
  [[ -z ${INPUT_NPM_VERSION:-} || ${INPUT_NPM_VERSION:-} =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail 'npm-version must be an exact stable version.'
  [[ ${INPUT_RUST_TOOLCHAIN:-stable} =~ ^[A-Za-z0-9._-]+$ ]] || fail 'Invalid rust-toolchain.'
  if [[ $GITHUB_EVENT_NAME == push ]]; then
    jq -e '.forced == false and .deleted == false' "${GITHUB_EVENT_PATH:?Event path is required}" >/dev/null || fail 'Forced or deleted pushes cannot run the release Action.'
  fi
}

validate_inputs
export RUSTUP_TOOLCHAIN=${INPUT_RUST_TOOLCHAIN:-stable}
if [[ ${1:-run} == validate ]]; then
  output operation "$operation"
  exit 0
fi
[[ ${1:-run} == run ]] || fail 'Unknown high-level Action entry point.'
scripts=$(cd -- "${BASH_SOURCE[0]%/*}" && pwd)
temp=$(mktemp -d)
work=''
trap cleanup EXIT
output release-commit ''
output releases '[]'
output published false
output published-packages '[]'
output publication-plan '[]'
output publication-needed false
output ready false
output has-notes false
output pr-number ''
output pr-operation none

run_script() {
  local name=$1
  shift
  : > "$temp/$name.outputs"
  GITHUB_OUTPUT="$temp/$name.outputs" bash "$scripts/$name.sh" "$@"
}

value() {
  local file=$1 key=$2
  # Each producer emits one-line JSON and scalar fields. Keep the last update.
  awk -v key="$key" 'index($0, key "=") == 1 { value = substr($0, length(key) + 2) } END { print value }' "$temp/$file.outputs"
}

copy_outputs() {
  local file=$1 key
  shift
  for key in "$@"; do output "$key" "$(value "$file" "$key")"; done
}

select_batch() {
  run_script preview
  copy_outputs preview ready publication-plan
  selected=$(value preview release-commit)
  local plan
  plan=$(value preview publication-plan)
  needed=$(jq -r 'any(.[]; .action != "none")' <<< "$plan")
  output publication-needed "$needed"
  if [[ -n $selected && ( $needed == true || ${INPUT_RETRY:-false} == true || $operation == retry-publication ) ]]; then
    output release-commit "$selected"
    output releases "$(value preview releases)"
  else
    selected=''
  fi
}

pr_result=0
if [[ $operation == auto || $operation == manage-pr ]]; then
  if [[ $operation == auto && ${INPUT_PR_ENVIRONMENT_READY:-true} == false ]]; then
    printf 'PR environment preparation failed; publication will still be attempted.\n' >&2
    pr_result=1
  else
    run_script version || pr_result=$?
    copy_outputs version has-notes pr-number pr-operation
  fi
  if [[ $operation == manage-pr ]]; then exit "$pr_result"; fi
fi
if [[ $operation == select || $operation == auto || $operation == retry-publication ]]; then
  select_batch
  if [[ $operation == select || -z $selected ]]; then exit "$pr_result"; fi
  output ready false
  run_script publish "$selected"
else
  run_script publish "${INPUT_EXPECTED_RELEASE_COMMIT}"
fi
copy_outputs publish release-commit releases published published-packages
output ready true
exit "$pr_result"
