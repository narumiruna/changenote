#!/usr/bin/env bash
# Root Action lifecycle. Commands run only for a verified managed merge.
set -euo pipefail
# shellcheck source=scripts/action/publication.sh
source "${BASH_SOURCE[0]%/*}/publication.sh"
scripts=$(cd -- "${BASH_SOURCE[0]%/*}" && pwd)
caller_root=$(git rev-parse --show-toplevel)
caller_token_set=${GH_TOKEN+x}
caller_token=${GH_TOKEN-}
export GH_TOKEN=${INPUT_GITHUB_TOKEN:?github-token is required}
output has-notes false
output pr-number ''
output pr-operation none
output release-commit ''
output releases '[]'
case ${GITHUB_EVENT_NAME:-} in
  push|workflow_dispatch) ;;
  *) fail 'Use the root Action on push or workflow_dispatch, not contributor PR events.' ;;
esac
for tool in uv python3; do
  command -v "$tool" >/dev/null || fail "Missing prerequisite: $tool"
done
setup_readonly
[[ ${GITHUB_REF:-} == "refs/heads/$base" ]] || fail 'Run the root Action on its selected base branch.'
export INPUT_BASE_BRANCH=$base
GITHUB_OUTPUT="$temp/route.outputs" UV_PYTHON_DOWNLOADS=never \
  uv run --no-project --no-config --python "$(command -v python3)" \
  python "$scripts/route-release.py" managed-merge
expected=$(awk -F= '$1 == "release-commit" {print $2}' "$temp/route.outputs")
if [[ $GITHUB_EVENT_NAME == push ]]; then
  after=$(jq -er '.after' "$GITHUB_EVENT_PATH")
  [[ ${GITHUB_SHA:-} == "$after" ]] || fail 'Push event does not match the triggering commit.'
  git merge-base --is-ancestor "$after" "$base_sha" || fail 'Triggering commit is not on the remote base branch.'
fi

# PR maintenance and merged-batch handling have independent results.
pr_result=0
: > "$temp/pr.outputs"
(cd -- "$caller_root"; GITHUB_OUTPUT="$temp/pr.outputs" bash "$scripts/version.sh") || pr_result=$?
while IFS='=' read -r key value; do
  case $key in has-notes|pr-number|pr-operation) output "$key" "$value" ;; esac
done < "$temp/pr.outputs"
[[ -n $expected ]] || exit "$pr_result"
cd -- "$root"
verify_publication_batch
[[ $release == "$expected" ]] || fail 'Release batch changed after routing; re-run the selected release workflow.'
has_commands=false
if [[ ${INPUT_PUBLISH_SCRIPT:-} =~ [^[:space:]] || ${INPUT_RELEASE_SCRIPT:-} =~ [^[:space:]] ]]; then
  has_commands=true
fi
if [[ $legacy_batch == true && $has_commands == true ]]; then
  fail 'Custom commands cannot publish historical note-bearing batches. Use matching old tooling.'
fi
selected=$release
releases=$(jq -c 'map({package, version, tag})' "$temp/batch.json")
output release-commit "$selected"
output releases "$releases"
[[ $has_commands == true ]] || exit "$pr_result"

# Worktrees share only this temporary repository, never the caller's Git config.
git -C "$root" remote add origin "$remote_url"
git -C "$root" config user.name 'github-actions[bot]'
git -C "$root" config user.email '41898282+github-actions[bot]@users.noreply.github.com'
git -C "$root" config core.hooksPath /dev/null
# Re-fetch after reconstruction, immediately before any caller command.
remote_git fetch --no-tags "$remote_url" "refs/heads/$base"
base_sha=$(git rev-parse FETCH_HEAD)
find_publication_batch
[[ $release == "$selected" ]] || fail 'Release batch changed before command execution; re-run the selected release workflow.'

run_command() {
  local name=$1 script_text=$2
  [[ $script_text =~ [^[:space:]] ]] || return 0
  [[ $(git -C "$work" rev-parse HEAD) == "$selected" ]] || fail 'A command changed the selected release HEAD; stop and inspect publication state.'
  printf 'Running %s.\n' "$name"
  printf '%s\n' "$script_text" > "$temp/$name.sh"
  (
    # Keep the management token out of the command environment. Caller-supplied
    # credentials remain available; this is not a sandbox for trusted commands.
    unset GH_TOKEN INPUT_GITHUB_TOKEN INPUT_PUBLISH_SCRIPT INPUT_RELEASE_SCRIPT BASH_ENV
    if [[ $caller_token_set == x ]]; then export GH_TOKEN="$caller_token"; fi
    export CHANGENOTE_RELEASE_COMMIT=$selected CHANGENOTE_RELEASES=$releases
    cd -- "$work"
    bash --noprofile --norc -e -o pipefail "$temp/$name.sh"
  )
}
run_command publish-script "${INPUT_PUBLISH_SCRIPT:-}"
run_command release-script "${INPUT_RELEASE_SCRIPT:-}"
exit "$pr_result"
