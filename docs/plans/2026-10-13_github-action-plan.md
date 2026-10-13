# GitHub Action implementation plan

## Goal

Provide a reusable GitHub Action in this repository that creates or updates one Release PR. After that PR is merged, publish annotated tags and GitHub Releases from the correct commit. Keep GitHub operations outside the Rust CLI.

## Context

- `src/lib.rs`: `status` returns package names, old and new versions, bumps, tags, and notes. `version` writes versions, changelogs, and `.changenote/releases.json`, then deletes notes.
- `tag` requires a clean checkout. Existing tags must point to the current commit. New tags point to the current commit.
- At the start of this work, `.github/workflows/release.yml` was manual and pushed version commits directly to `main`.
- The saved release batch has no commit identity or publication state. Its presence alone does not mean a release is pending.
- Reference: `/home/narumi/src/action/src/run.ts` manages a fixed release branch and creates or updates its PR; `src/github.ts` manages Git operations and authentication; `src/select-mode/index.ts` separates versioning from publication.
- At the start of this work, there was no Action implementation or Node package in this repository.

## Architecture

Use composite Actions with small Bash scripts, Git, `gh`, and `jq`. The first supported runner is Linux. Do not introduce TypeScript or a Node build unless the discovery checks show that the composite design cannot meet the requirements.

- Root `action.yml`: create or update a Release PR.
- `publish/action.yml`: publish the latest committed release batch from its release commit.
- `scripts/action/`: implementation scripts grouped by responsibility.
- `.github/workflows/release.yml`: run PR management and then publication in separate jobs on pushes to `main` and manual dispatch. Each job has its own minimum permissions. Manual runs also provide publication recovery. The separate retry workflow was removed at the user's request.

Implementation correction: GitHub permits only one pending run per concurrency group and can replace that pending run even with `cancel-in-progress: false`. Two push-triggered workflows could therefore cancel one of the required operations. The single Release workflow contains both jobs and also supports manual retries. This preserves the goal and strengthens lifecycle safety.

The Rust CLI remains responsible for note validation, version changes, changelogs, release plans, and local annotated tags.

The Release workflow uses one concurrency group with `cancel-in-progress: false`. Each run fetches the current base branch after it acquires the lock; an older queued event must not replace a newer PR state.

The PR Action requires a checked-out repository and an installed `changenote` binary. It does not install or build the CLI. In this repository, the workflow builds the CLI. External repositories install a pinned CLI revision. Document this requirement rather than adding an installation subsystem.

### Initial interface

PR Action inputs: `github-token`, `base-branch`, `commit-message`, and `pr-title`. Defaults use the workflow token, the default repository branch, and Conventional Commit release titles. Use one derived bot-owned branch in the `narumi/chore/` namespace; do not add a branch override in the first implementation.

PR Action outputs: `has-notes`, `pr-number`, and `pr-operation` (`created`, `updated`, `closed`, or `none`).

Publication Action inputs: `github-token` and `base-branch`. Outputs: `release-commit`, `published`, and `published-packages`. `published` means that this run created at least one remote tag or GitHub Release; it does not mean registry publication.

Pass authentication explicitly to both Git and `gh`. Do not change Git `user.name` or `user.email`; use command-scoped author and committer environment variables. Do not rely on credentials persisted by checkout.

### Release identity and publication state

Discover the latest commit on the base branch's first-parent history that changes `.changenote/releases.json`. For a merged Release PR, use the resulting base-branch merge or squash commit, not the pre-merge PR head. Validate the batch and configured version files at that commit and publish from a clean checkout of it.

Do not use the workflow event SHA or the latest base-branch HEAD as an implicit release target. A later source-only commit must not change the target.

Remote tags and GitHub Releases determine publication state. A tag at a different commit is an error. Missing publication objects can be created on retry; existing correct objects are left unchanged. New pending notes do not prevent recovery of an earlier release batch.

## Non-Goals

- Registry publication, multi-platform binary distribution, or automatic CLI installation.
- General manifest synchronization, dependency propagation, prereleases, or linked version groups.
- Draft PR controls, custom version scripts, GitHub API commit signing, or parity with all Changesets Action options.
- Automatic merge or approval of Release PRs.

## Plan

### 1. Verify the integration contracts

- [x] Inspect composite Action path resolution, Linux tool availability, and checkout credential behavior; confirm that scripts run against the consumer repository while loading code from `github.action_path`. Evidence: a minimal fixture invocation with the documented prerequisites.
- [x] Test release commit discovery with merge commits, squash merges, rebase merges, later unrelated commits, and manual edits to the batch file. Establish fail-closed validation for malformed batches and inconsistent versions. Evidence: Git fixture tests with explicit expected commit IDs.
- [x] Fix the initial JSON output and input contracts in Action metadata and tests. Determine whether existing CLI output needs changes; prefer the current release plan over a new CLI command or schema. Evidence: fixture plans cover multiple packages and multiline notes.

### 2. Implement Release PR management

- [x] Add root `action.yml` and scripts that validate prerequisites, token permissions, a clean checkout, and the current remote base branch before changing files. Evidence: missing tools, invalid notes, dirty checkouts, and failed fetches stop without pushing changes.
- [x] Read `changenote status` and apply `changenote version` only when the plan is nonempty. Stage only consumed note paths, affected version files and changelogs, and `.changenote/releases.json`. Evidence: an unrelated generated file is not committed.
- [x] Rebuild the bot-owned release branch from the current remote base, then push with an expected remote SHA using `--force-with-lease`. Reject an existing branch that is not associated with the managed Release PR, unless a prior managed PR establishes ownership. Evidence: updates retain one branch; concurrent branch changes and ownership conflicts stop safely.
- [x] Create or update one PR using its head and base branches. Generate a body with old/new versions, bumps, and notes; keep the body within GitHub limits. Pass summaries as file contents, not shell source. Evidence: create, update, oversized body, shell metacharacter, and multiline fixtures pass.
- [x] Make reruns with the same generated tree a no-op for commits and pushes. When notes disappear, close only the managed stale PR; do not delete unrelated branches. Evidence: rerun, note removal, manually closed PR, and merged-PR fixtures have defined outcomes.
- [x] Emit the documented outputs after successful operations. Evidence: tests verify both PR and no-notes output values.

### 3. Implement publication and recovery

- [x] Add `publish/action.yml` and release discovery scripts that fetch the latest base branch and inspect remote tags, select the validated release commit, and use a separate clean worktree for publication. Keep the CLI executable outside a checkout that can change revisions. Evidence: later unrelated commits do not become tag targets and no committed batch produces a no-op.
- [x] Inspect every planned remote tag before publication. Accept only tags whose peeled commit matches the selected release commit; report conflicting tags without changing them. Run `changenote tag` in the release worktree to create missing annotated tags. Evidence: correct, missing, and conflicting tags pass their expected cases.
- [x] Push each annotated tag explicitly and create missing GitHub Releases from the batch notes. Verify existing release tag identity and leave existing release text unchanged. Recheck remote state after a push race rather than treating any push failure as success. Evidence: partial tag pushes and partial release creation can be retried without duplicates.
- [x] Distinguish pending, completed, and conflicting publication states. Do not infer publication from an empty note plan. Emit the release commit and packages affected by the current run. Evidence: completed batches, pending notes alongside an older batch, and retries after later source commits pass.

### 4. Integrate repository workflows and documentation

- [x] Add the Release PR workflow and replace the manual direct-push release flow with publication-only execution. Set explicit permissions, a shared concurrency group, and a trusted base-branch trigger. Do not use `pull_request_target` to execute contributor code. Evidence: workflow validation confirms PR jobs need `contents: write` and `pull-requests: write`; publication needs only `contents: write`.
- [x] Update `README.md` with pinned Action and CLI usage, Linux prerequisites, outputs, managed branch behavior, permissions, repository settings, recovery limits, and App-token guidance for PR CI. State that the Action does not publish registries or synchronize manifests. Evidence: examples match metadata and run in the fixture repository.
- [x] Add Action tests to CI without removing the Rust and Python checks. Evidence: shell syntax checks, YAML validation, mocked GitHub tests, and local Git integration tests pass.
- [x] Add one `minor` changenote note for the reusable Action and automatic release behavior, then validate status. Evidence: `cargo run --locked -p changenote -- status` reports the intended package and bump without validation errors.

### 5. Validate the hosted workflow

- [ ] With user authorization and a disposable GitHub repository, test PR creation, note additions updating the same PR, PR merge, tag targets, GitHub Release creation, and retry after a later commit. Also test CI triggering with an App token. Evidence: recorded run URLs, PR number, and tag commit IDs. Leave this step open if credentials or authorization are unavailable.

## Verification evidence

- Branch: `narumi/feat/github-release-action`, based on `origin/main`. The initial untracked plan was preserved and updated; no unrelated source changes were present.
- `tests/actions/test_actions.py`: 31 passing local Git/mock GitHub tests. Cases include PR updates, closed/merged PRs, no-op reruns, branch leases and no-op races, partial pushes, release failures, tag races, root/malformed/edited batches, shallow/stale checkouts, multiple packages, and shell metacharacters.
- `tests/action_metadata/test_metadata.py`: 3 passing tests. They run the composite commands from a consumer repository with no Action source and verify metadata, README examples, workflow triggers, concurrency, permissions, and outputs.
- Bash syntax checks for each script, ShellCheck with sources, actionlint v1.7.11, Python test-source lint checks, and `uvx pre-commit run --all-files` pass.
- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` (12 integration tests), and `cargo build --locked --release` pass.
- `uv sync --locked`, CLI status and module help, Python wrapper tests (10), wheel/source build, and isolated local-wheel `uvx` help checks pass.
- `.changenote/github-release-action.md` requests `minor` for `changenote`. Status validation passes; the combined repository plan remains `major` because of an existing Python compatibility note.
- Diff review checked explicit token use, trusted triggers, scoped staging, unchanged CLI contracts, worktree isolation, branch ownership, immutable tags, partial failure recovery, and pending-run lifecycle.
- Initial PR CI run `38415703223` found an implicit remote-HEAD dependency in shallow fetches. Fetch now explicitly names the base ref, and all fixture remotes have an unborn default HEAD. It also exposed an existing Python path assertion failure on macOS/Windows (already present in main run `38409234152`). The assertion now compares resolved paths without changing wrapper behavior; local tests also pass with a symlinked temporary directory.
- Hosted smoke tests remain blocked: the user prohibits package publication, version tags, and release workflow dispatch. No disposable repository or GitHub App token was supplied. Local tags exist only in temporary test repositories; no GitHub release or repository tag was created. The plan remains open and must not be deleted.

## Risks

- First-parent batch discovery depends on the base branch's history. History rewrites can remove the release identity. Stop on conflicts; do not move tags to repair them.
- The managed branch must be reserved for automation. Lease-based pushes prevent overwriting concurrent changes but do not replace ownership checks.
- `VERSION` does not synchronize `Cargo.toml` or lockfiles. Tags and GitHub Releases are in scope; registry publication must remain disabled until the actual manifests and artifacts are aligned.
- GitHub permissions, branch rules, and token event behavior require hosted validation. Mocked tests alone cannot prove those settings.

## Rollback / Recovery

Disable the Release workflow to stop automation. Close the managed Release PR if necessary; unmerged PR version changes do not affect the base branch.

After a release PR has merged, retry publication from the discovered release commit. Existing correct tags and releases remain unchanged. If a tag points to another commit, stop and require manual review. Do not delete releases, move tags, or roll back published versions automatically.

Implementation and test runs must not execute a real repository `version`, `tag`, or Release workflow without explicit user authorization for that release action. Use temporary fixture repositories for version and tag tests.

## Completion Checklist

- [x] Release PR creation and updates are reusable from a consumer repository without the changenote source tree.
- [x] No-op reruns, managed-branch ownership, lease conflicts, and bounded PR bodies are covered by tests.
- [x] Merge, squash, and rebase fixtures produce tags on the documented release commit, including retries after later source commits.
- [x] Partial publication is recoverable; conflicting tags are never moved.
- [x] Action metadata, README examples, workflow permissions, and outputs agree.
- [x] Action checks and `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked` pass.
- [x] The minor release note passes `cargo run --locked -p changenote -- status`.
- [ ] Hosted checks pass, or their external prerequisites are explicitly accepted as a remaining handoff condition.
- [ ] Delete this plan only after implementation checks and required handoff are complete.
