# Check PR notes

Check committed PR changes without changing files.
The CLI defaults to PR-level coverage; use `--coverage package` for direct notes on every changed package.
The check Action defaults to package coverage.

- [CLI check](#cli-check)
- [File patterns for note checks](#file-patterns-for-note-checks)
- [GitHub Action report](#github-action-report)

## CLI check

```sh
changenote check --since origin/main
changenote check --since origin/main --coverage package
changenote check --since origin/main --skip-release --reason "Documentation only"
```

Set optional `base_branch` in `.changenote/config.json` to select a default comparison ref, for example `"base_branch": "origin/main"`.
An explicit `--since` takes priority. If neither is present, the CLI reports a missing argument.
The selected ref must exist locally; the CLI does not fetch, guess a branch, or change refs.
An unused configured ref need not exist, but its configuration value must still be valid.
This setting does not select the Release PR or publication target; Action inputs remain authoritative.
Rust callers can use `configured_base_branch()`; existing explicit-ref check APIs remain supported.
`load_config()` validates this setting but does not retain it in the primary-package view.

`check` requires the Git repository root, a clean working tree, full enough history to find a merge base, and an available base ref. It compares tracked paths between `git merge-base <ref> HEAD` and `HEAD`; it does not include uncommitted or untracked changes. File renames report both old and new paths. Nested packages use the deepest matching configured directory. Files outside all configured packages are reported but do not imply a package release.

If packages changed, at least one valid top-level note must be added, modified, or renamed in that comparison and remain in `HEAD`. Unchanged old notes and deleted notes do not satisfy the check. The default `--coverage pr` is a **PR-level check, not proof of per-package note coverage**. No file patterns are excluded automatically. Optional `changed_file_patterns` can limit which package-relative paths require notes (see below). Use the explicit exemption for documentation-only or test-only changes inside package directories. `--skip-release` requires a non-empty `--reason`; it never bypasses config, version, note, or Git validation.

Use `--coverage package` to require a direct package assignment in a changed note for every changed package.
A note for another package, an unchanged old note, or fixed-group propagation does not cover that package.
One changed note can cover several packages.
Strict reports add sorted `covered_packages` and `uncovered_packages` arrays for changed packages only.
An explicit exemption can pass with uncovered packages; it does not hide those arrays or bypass validation.
Default reports and the Rust `check()` API remain unchanged; Rust callers can use `check_with_coverage()`.

The command writes no files. It prints a JSON report with `merge_base`, `changed_packages`, `note_paths`, `outside_paths`, `skip_reason`, and `passed` when validation succeeds, including a missing-note failure. Exit codes are `0` for a passing report, `1` for a missing note or validation/runtime error, and `2` for invalid CLI arguments. Validation errors write diagnostics to stderr and do not print a report.

A consumer CI job can use the following steps. Replace the SHA with a reviewed compatible CLI revision. Exempt PRs must use an explicit repository review policy; this job does not infer exemptions.

```yaml
permissions:
  contents: read
steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
      persist-credentials: false
  - uses: dtolnay/rust-toolchain@stable
  - name: Install changenote
    run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
  - name: Check notes
    env:
      BASE_BRANCH: ${{ github.base_ref }}
    run: changenote check --since "refs/remotes/origin/$BASE_BRANCH"
```

Use this job for `pull_request` events. Checkout must contain `refs/remotes/origin/<base branch>` and a clean tree. Branch names enter the shell through an environment variable, not inline expression substitution.

For tested, isolated fixtures, see [changenote-example](https://github.com/narumiruna/changenote-example). Use its pinned CLI revision; released binaries can have different features.

## File patterns for note checks

```json
{
  "changed_file_patterns": ["**", "!docs/**", "!tests/**"]
}
```

Omission includes all tracked package files. A supplied list must be nonempty and have an inclusion pattern.
Match a complete package-relative path, with forward slashes and case-sensitive matching.
An inclusion must match and no exclusion may match; a leading `!` marks an exclusion. List order cannot re-include a path.
`*` and `?` do not cross path separators. A complete `**` component matches zero or more directories; trailing `**` also matches descendant files.
Bracket classes and brace alternatives are supported. Wildcards match leading dots.
Reject malformed patterns, absolute or drive paths, backslashes, empty/current/parent components, and partial `**` components.
This is not Git ignore syntax. There are no implicit file exclusions.

The deepest package owns each path before filtering. An excluded nested path never falls back to a parent package.
Renames check old and new paths separately; deletions use the old path.
Filtered in-package paths are not reported as outside paths.
Patterns affect both coverage modes, but never filter pending releases or hide configuration, version, note, or Git errors.
Changed notes are discovered independently, even when the note path does not match.
Upgrade CLI and check Action pins together before using this setting.

## GitHub Action report

Use the check Action after checkout and installation of a reviewed compatible CLI:

```yaml
permissions:
  contents: read
steps:
  - uses: actions/checkout@v7
    with:
      fetch-depth: 0
      persist-credentials: false
  - uses: dtolnay/rust-toolchain@stable
  - name: Install trusted CLI
    run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
  - uses: narumiruna/changenote/check@<full-action-commit-sha>
    with:
      since: refs/remotes/origin/${{ github.base_ref }}
      coverage: package
```

Run this job on `pull_request`, not a privileged event that executes contributor code.
The Action requires Linux, Bash, Git, `jq`, an installed trusted CLI, a clean repository root, and sufficient local history.
It does not fetch, build contributor code, run package managers, post comments, or require a write token.
Do not install the CLI from the PR revision. The checkout step needs read permission; the reporting script uses no GitHub token.

The required `since` input names an available Git ref. `coverage` defaults to `package`; set `pr` for the original policy.
An optional `skip-release: 'true'` requires a nonempty `reason` from the repository's review policy.
A reason is invalid without that exemption. The Action does not infer exemptions from labels or commit types.

Outputs are `check-report` (PR comparison JSON), `release-plan` (all pending release JSON), and `passed` (`true` or `false`).
The job summary and annotations distinguish the PR check from the complete pending plan, which can include older notes and fixed-group releases.
A missing-note failure emits validated reports and fails the step. Configuration, history, CLI, or report-validation errors fail without report outputs.
No output is a publication-readiness result. Summary text is escaped, including line breaks in paths and exemption reasons; note text is never evaluated as a command.

Local fixtures cover the report and permission contract. Hosted fork events and repository branch rules remain unverified for this Action.
A maintainer must configure and test those settings in an authorized disposable repository before relying on the hosted workflow.
No hosted test, tag creation, or publication is authorized by the local checks.
