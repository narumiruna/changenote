#!/usr/bin/env bash
# Read-only PR reporting. The caller supplies a trusted CLI and committed checkout.
set -euo pipefail

annotation() {
  local level=$1 message=$2
  message=${message//%/%25}
  message=${message//$'\r'/%0D}
  message=${message//$'\n'/%0A}
  # Do not put contributor paths in annotation properties.
  printf '::%s::%s\n' "$level" "$message"
}
fail() { annotation error "$*"; exit 1; }

[[ $(uname -s) == Linux ]] || fail 'Only Linux runners are supported.'
for tool in git jq changenote mktemp; do
  command -v "$tool" >/dev/null || fail "Missing prerequisite: $tool"
done
[[ -n ${GITHUB_OUTPUT:-} ]] || fail 'GITHUB_OUTPUT is required.'
[[ -n ${INPUT_SINCE:-} ]] || fail 'An explicit since input is required.'
coverage=${INPUT_COVERAGE:-package}
case $coverage in package|pr) ;; *) fail 'coverage must be package or pr.' ;; esac
skip=${INPUT_SKIP_RELEASE:-false}
reason=${INPUT_REASON:-}
case $skip in
  true) [[ $reason =~ [^[:space:]] ]] || fail 'skip-release requires a nonempty reason.' ;;
  false) [[ -z $reason ]] || fail 'reason requires skip-release=true.' ;;
  *) fail 'skip-release must be true or false.' ;;
esac
[[ -z ${GIT_INDEX_FILE:-} && -z ${GIT_WORK_TREE:-} && -z ${GIT_DIR:-} ]] || fail 'Custom Git worktree variables are not supported.'
export GIT_OPTIONAL_LOCKS=0
# The CLI enforces a clean repository root and validates config, notes, and history.
temp=$(mktemp -d)
trap 'rm -rf -- "$temp"' EXIT
args=(check "--since=$INPUT_SINCE" "--coverage=$coverage")
if [[ $skip == true ]]; then args+=(--skip-release "--reason=$reason"); fi
check_result=0
changenote "${args[@]}" > "$temp/check.json" 2> "$temp/check.err" || check_result=$?
if [[ -s $temp/check.err ]]; then annotation warning "$(< "$temp/check.err")"; fi

# A runtime/validation failure is not a negative coverage report. Never emit
# readiness outputs for empty, multiple, malformed, or inconsistent reports.
if ! jq -e -s --arg coverage "$coverage" --argjson result "$check_result" '
  def strings: type == "array" and all(.[]; type == "string");
  length == 1 and (.[0] |
    type == "object" and
    (.merge_base | type == "string" and test("^[0-9a-f]{40,64}$")) and
    (.changed_packages | strings) and (.note_paths | strings) and
    (.outside_paths | strings) and has("skip_reason") and
    (.skip_reason | . == null or type == "string") and
    (.passed | type == "boolean") and (.passed == ($result == 0)) and
    (if $coverage == "package" then
      (.covered_packages | strings) and (.uncovered_packages | strings)
     else true end))
' "$temp/check.json" >/dev/null 2>&1; then
  annotation error 'The CLI did not return a valid PR check report. No report outputs are available.'
  if (( check_result != 0 )); then exit "$check_result"; else exit 1; fi
fi

# This is intentionally not restricted to the PR diff: older pending notes and
# fixed-group propagation are part of the complete release plan.
status_result=0
changenote status --include-note-paths > "$temp/plan.json" 2> "$temp/status.err" || status_result=$?
if [[ -s $temp/status.err ]]; then annotation warning "$(< "$temp/status.err")"; fi
if (( status_result != 0 )) || ! jq -e -s '
  def strings: type == "array" and all(.[]; type == "string");
  length == 1 and (.[0] | type == "array" and all(.[];
    type == "object" and (.package | type == "string") and
    (.old_version | type == "string") and (.version | type == "string") and
    (.tag | type == "string") and (.bump | . == "major" or . == "minor" or . == "patch") and
    (.notes | strings) and (.note_paths | strings) and
    (.notes | length) == (.note_paths | length)))
' "$temp/plan.json" >/dev/null 2>&1; then
  annotation error 'The CLI did not return a valid complete pending release plan. No report outputs are available.'
  if (( check_result != 0 )); then exit "$check_result"; else exit 1; fi
fi

passed=$(jq -r '.passed' "$temp/check.json")
if [[ -n ${GITHUB_STEP_SUMMARY:-} ]]; then
  {
    printf '## PR note check\n\nCoverage: %s. Passed: %s.\n\n' "$coverage" "$passed"
    printf 'This report covers only committed changes since the selected merge base.\n\n'
    jq -r '
      def scalar: @html | gsub("\r"; "&#13;") | gsub("\n"; "&#10;");
      def items: if length == 0 then "None" else map(scalar) | join(", ") end;
      "<p>Merge base: <code>" + (.merge_base | scalar) + "</code></p>",
      "<table><tr><th>PR check field</th><th>Result</th></tr>",
      "<tr><td>Changed packages</td><td>" + (.changed_packages | items) + "</td></tr>",
      (if has("uncovered_packages") then
        "<tr><td>Packages without direct changed notes</td><td>" + (.uncovered_packages | items) + "</td></tr>"
       else empty end),
      "<tr><td>Changed notes</td><td>" + (.note_paths | items) + "</td></tr>",
      "<tr><td>Paths outside configured packages</td><td>" + (.outside_paths | items) + "</td></tr>",
      "<tr><td>Explicit exemption</td><td>" + ((.skip_reason // "None") | scalar) + "</td></tr></table>"
    ' "$temp/check.json"
    printf '\n## All pending releases\n\nThis complete plan includes older pending notes and fixed-group releases, not only this PR. It is not publication readiness.\n\n'
    # Escape each value before putting it in HTML. Note text is inside pre
    # blocks, so arbitrary Markdown, HTML, and backticks remain inactive data.
    jq -r '
      def scalar: @html | gsub("\r"; "&#13;") | gsub("\n"; "&#10;");
      if length == 0 then "No pending releases."
      else
        "<table><tr><th>Package</th><th>Current version</th><th>Next version</th><th>Bump</th><th>Tag</th></tr>",
        (.[] | "<tr>" + ([.package, .old_version, .version, .bump, .tag] |
          map("<td>" + scalar + "</td>") | join("")) + "</tr>"),
        "</table>",
        (.[] | "<h3>" + (.package | scalar) + "@" + (.version | scalar) + "</h3>",
          (if has("linked_group") then "<p>Linked version baseline: <code>" + (.linked_group.base_version | scalar) + "</code>. Direct notes remain with their named package.</p>" else empty end),
          (if (.notes | length) == 0 then
             "<p>No direct notes. This package is released with its fixed version group.</p>"
           else . as $release | range(0; .notes | length) as $index |
             "<p>Note: <code>" + ($release.note_paths[$index] | scalar) + "</code></p><pre>" +
             ($release.notes[$index] | @html) + "</pre>"
           end))
      end
    ' "$temp/plan.json"
  } >> "$GITHUB_STEP_SUMMARY"
fi
{
  printf 'check-report=%s\n' "$(jq -c . "$temp/check.json")"
  printf 'release-plan=%s\n' "$(jq -c . "$temp/plan.json")"
  printf 'passed=%s\n' "$passed"
} >> "$GITHUB_OUTPUT"
if [[ $passed == true ]]; then
  annotation notice "PR note check passed ($coverage coverage). The pending release plan is separate."
else
  annotation error "PR note check failed ($coverage coverage). Changed packages: $(jq -r '.changed_packages | join(", ")' "$temp/check.json"). Add the required notes or use a reviewed explicit exemption."
fi
exit "$check_result"
