# GitHub release Actions

## Root Action: post-merge commands

Use `narumiruna/changenote@<full-action-commit-sha>` to manage version PRs and run optional project commands after a managed PR is merged.
See [the command-hook workflow](../examples/github-actions/command-hooks.yml).
Replace all SHA placeholders with reviewed compatible full commit SHAs.
The root Action needs an installed trusted CLI, Bash, Git, `gh`, `jq`, and Python 3 on Linux. Ubuntu runners provide the non-CLI tools.
It sets up uv for its own router, not the project's runtime or dependencies.
Prepare supported lockfile tools and caches for PR generation and release replay as described in the README.
Never build the trusted CLI from a contributor PR.

The existing inputs `github-token`, `base-branch`, `commit-message`, and `pr-title` retain their defaults.
The token needs contents and pull requests write permissions for PR management, including read access for merge association.
`base-branch` defaults to the repository default branch.
The Action never approves or merges its PR.
If generated PRs must start CI, use a GitHub App token or PAT through `github-token`; the default workflow token does not start PR workflows.

| Optional input | Default | Execution |
| --- | --- | --- |
| `publish-command` | Empty | Publish packages after managed-merge and full-tree verification. |
| `release-command` | Empty | Create tags, assets, or a GitHub Release after successful `publish-command`, or directly if it is absent. |

Commands are Bash script text. Whitespace-only input is absent.
For example, use `publish-command: uv publish` for prepared Python artifacts or `publish-command: cargo publish --locked` for a prepared crate.
A project script can test, build, and upload in one command.
With both inputs, the fixed order is **publish, then release**. With one input, only that command runs.
With neither input, the root Action creates no tags, GitHub Releases, or registry uploads.
There are no built-in registry integrations or command-order options.

On a normal base-branch push, the router checks only new batch-changing first-parent commits in that push range.
It selects the latest batch and requires exactly one associated merged PR with the reserved same-repository release branch, selected base, managed body marker, and matching merge commit.
Titles and commit messages do not authorize command execution.
Merge, squash, and rebase merges are supported.
Contributor PR events, wrong branches, forced/deleted pushes, missing history, invalid associations, and API failures stop the root Action before commands.
Manual `workflow_dispatch` only manages the PR, regardless of its inputs.
An unrelated later push does not run commands for an old batch.
If one push contains multiple release batches, only the latest is selected; finish one release before merging the next.

Before execution, the Action reconstructs the consumed-note plan and checks the complete generated tree.
It fetches the base branch again after reconstruction and rejects a changed batch selection.
It does not switch silently to another release commit.
This check does not reserve the branch against a later merge.
GitHub tag/Release state does not decide whether commands run. Changelog output can be disabled.
Historical note-bearing batches remain read-only; custom commands are rejected for them.
The stricter legacy, changelog-body, and remote-publication rules of `/preview` and `/publish` remain unchanged.

Commands run once per batch per eligible invocation in a disposable worktree at the verified release commit.
They share generated files between commands and have an `origin` remote without stored management credentials.
Commands must not change HEAD; a changed HEAD prevents the next command.
Git pushes need caller-provided Git authentication; setting `GH_TOKEN` alone does not configure a Git credential helper.
The caller's tracked files, index contents, HEAD, and Git identity are not replaced by the hook runner.
PR management retains its normal fetch behavior.
Untracked files from earlier workflow steps are not copied into that worktree.
Build artifacts there, or explicitly use an artifact directory downloaded by the workflow.
Temporary trees are removed after success or failure, so retain artifacts before the command exits if retries require identical bytes.

Both commands receive:

- `CHANGENOTE_RELEASE_COMMIT`: the verified full commit SHA.
- `CHANGENOTE_RELEASES`: a compact JSON array with `package`, `version`, and `tag` for every package in the batch.
  - Read this array for a monorepo. There is no single global version variable.

The caller supplies trusted command text, runtimes, dependencies, artifacts, credentials, and job permissions.
Do not construct command inputs from PR titles, notes, or other contributor data.
The `github-token` input is not automatically exported to commands.
Pass needed credentials explicitly through the Action step's `env`, such as `GH_TOKEN` for a caller-owned GitHub Release script or the registry's required variable.
A command runs with the job's authority. This is not a security sandbox, and caller-provided secrets remain accessible to trusted code.
Grant `id-token: write` only when the configured publication method needs it.

Project tests, artifact checks, and a package dry run must pass before an upload.
The publication script can enforce those checks at the verified commit.
Metadata verification is not proof that project CI passed.
For separate CI jobs, native runners, independent deployment approvals, or cross-platform artifacts, keep the external-CI structure below rather than compress it into a single command job.
A composite Action cannot set job concurrency or call a reusable workflow as a job.
Configure caller concurrency with `cancel-in-progress: false`, but do not treat it as a durable FIFO queue: newer pending runs can replace older pending runs.

The root retains `has-notes`, `pr-number`, and `pr-operation` outputs.
It adds `release-commit` and `releases` for the verified merged batch, even when both commands are absent.
No selected merge returns an empty commit and `[]`.
Read outputs only after a successful Action step; command success alone does not prove publication state.
A PR-maintenance failure does not suppress an independently verified merged batch, but the Action still reports failure.

### Hook retries and migration

A command failure fails the Action. A failed `publish-command` prevents `release-command`.
The Action does not retry commands or undo external writes.
Re-running the original eligible workflow repeats configured commands from the beginning, even when a GitHub Release already exists.
Scripts must handle already-published versions and partial uploads safely.
If a newer release batch has replaced the selected batch, the old run fails instead of publishing a different version.
Inspect state and recover explicitly; do not move tags or replace published packages automatically.

Consumers of the older root Action must restrict invocation to base-branch `push` and `workflow_dispatch` events.
Do not use the root on contributor PRs. Use `/check` for read-only note checks.
The compatible installed-CLI prerequisite and existing PR-input defaults remain unchanged.
Both commands are opt-in; upgrading without command inputs does not enable package or GitHub publication.
Consumers using `/release` retain its existing built-in publication and historical-recovery behavior.
No consumer or production workflow is migrated by these examples.

### Hosted handoff and recovery

Local Git/mock tests do not establish release readiness.

| Responsible role | Prerequisite and action | Expected result and unverified path | Restriction |
| --- | --- | --- | --- |
| Repository maintainer | After separate authorization, configure a disposable repository, reviewed Action SHA, branch rules, token, and environments. Merge a managed PR, then re-run its original workflow. | Confirm hosted event delivery, merge association, command order, token scopes, branch permissions, and pending-run behavior. | Do not test with a production Release workflow. |
| Registry/project owner | After artifact review and specific upload authorization, configure credentials or Trusted Publishing and perform a controlled upload followed by the project release step. | Confirm registry ownership, OIDC configuration, deployment approvals, and actual publication. | Dummy-command tests authorize no upload. Never send secrets through chat. |

To stop custom publication, disable the consumer workflow or remove both commands; cancel active runs separately.
Restore the previous reviewed Action SHA and matching workflow to roll back code.
Do not edit saved release batches or move existing tags.
After a partial upload, inspect registry, tag, and Release state before retrying.
Reuse original artifacts if rebuilt bytes would conflict. Fix a faulty published package with a new version.
Yanks and asset removal require separate maintainer authorization.

## Config-driven GitHub publication

Use `narumiruna/changenote/release@<full-action-commit-sha>` to install the CLI and run releases.
Keep package settings in `.changenote/config.json`. Select workflow behavior and tool versions with Action inputs.

The existing root, `check`, `preview`, and `publish` Actions remain supported.
They do not install the CLI. The new `release` Action is an opt-in entry point.

## Minimal workflow

Copy [the minimal workflow](../examples/github-actions/release.yml) to `.github/workflows/release.yml`.
Replace its SHA placeholder with a reviewed full Action commit SHA.
No separate CLI version pin, Rust installation, or publication script is needed.

The default `auto` operation manages the Release PR and publishes incomplete validated batches on ordinary pushes.
It can complete an older batch after later pushes, like the lower-level publication Action.
It does not require a new batch in the current push or an associated merged managed PR.
It does not run project CI.
Use the separate operations below when CI must validate the exact release commit before publication.

Manual runs default to `manage-pr`, which cannot publish.
Select `retry-publication` explicitly to retry the latest validated batch without managing the PR.
The Action never merges its PR.

Before use:

1. Create and commit the package configuration and version files.
2. Enable **Actions > General > Allow GitHub Actions to create and approve pull requests**.
3. Permit the bot to push the managed release branch.
4. If Release PRs must trigger CI, pass a GitHub App token or PAT through `github-token`.
   - PRs created with the default `GITHUB_TOKEN` do not automatically trigger PR workflows.
5. Use Ubuntu runners and the selected base branch for push and manual events.
   - Contributor PR events, forced pushes, and deleted-branch pushes are rejected.

Repository settings and token permissions cannot be supplied by package configuration.
Do not merge another Release PR until the current batch and downstream artifacts are complete.

## Tool setup

The Action builds the CLI from its own selected source and `Cargo.lock`.
It never builds a CLI from the consumer repository.
A successful run caches the executable by source content, runner platform, architecture, and Rust toolchain.
Cold caches need access to Rust distribution servers and crates.io.

For each release operation, the Action reads validated configuration in temporary checkouts.
It discovers primary and additional manifest outputs, then finds their nearest supported lockfile.
Shared lockfiles are prepared once per revision.
Plain-text versions, manifests without lockfiles, and inactive release plans need no package-cache preparation.

Selection and publication use the selected batch's parent configuration, not only the latest branch configuration.
Automatic operation also prepares the current pending PR plan.
This supports historical releases after configuration changes.

| Manifest with existing lockfile | Cache preparation |
| --- | --- |
| `Cargo.toml` | `cargo fetch` |
| `package.json` | `npm install --ignore-scripts --no-audit --no-fund` |
| `pyproject.toml` | `uv sync --no-install-project --no-install-workspace --no-build` |

Preparation runs online in temporary checkouts and can refresh their temporary lockfiles.
This supports historical batches whose manifest versions changed without lockfile synchronization.
The selected Rust toolchain overrides consumer toolchain files for setup and replay.
uv uses a private temporary environment, not the consumer's configured environment or `.venv` symlink.
Version writes and publication replay retain their offline commands.
npm lifecycle scripts are disabled. uv project installation and source builds are disabled.
Preparation does not change the consumer checkout.
The Action removes `GH_TOKEN` and `GITHUB_TOKEN` from package-cache commands.
Configure private package credentials through the package manager's normal configuration.
Do not depend on the Action's write token for dependency downloads.

Use the same Node.js, npm, Rust, and uv versions for PR generation, selection, and publication.
Version differences can change lockfiles and cause full-tree verification to fail.
Pin tool versions when reproducible lockfiles are required.
The existing restrictions on unsupported lockfiles and dynamic manifest versions still apply.

## Operations and inputs

| `operation` | Behavior | Token permissions |
| --- | --- | --- |
| `auto` (default) | On push, manage the PR and publish pending validated work. On manual events, manage the PR only. | Contents write, pull requests write |
| `manage-pr` | Create, update, or close the managed Release PR. | Contents write, pull requests write |
| `select` | Inspect the latest batch without Git or GitHub writes; select it only when publication work remains. | Contents read |
| `publish` | Publish only the exact commit passed through `expected-release-commit`. | Contents write |
| `retry-publication` | On a manual event, publish the latest validated batch, including an already completed batch. | Contents write |

`auto` still attempts historical publication if current-plan preparation or PR management fails, then reports the PR error.
CLI installation, tool installation, and historical-environment failures stop the Action.
Use separate jobs if downstream work must be independent of PR-management failure.
All publication paths reuse full-tree replay, changelog validation, conflict detection, and remote race checks.
They create Git tags and GitHub Releases, not registry artifacts.

| Input | Default | Rule |
| --- | --- | --- |
| `github-token` | `github.token` | Use the permissions required by the operation. |
| `base-branch` | Repository default branch | The triggering ref must be this branch. |
| `expected-release-commit` | Empty | Required for `publish`; full lowercase 40-character commit SHA. Rejected for other operations. |
| `retry` | `false` | With `select` on a manual event, include completed batches for downstream retries. |
| `node-version` | `lts/*` | Used when a configured npm lockfile needs preparation. |
| `npm-version` | Bundled npm | Optional exact stable version, such as `11.20.0`. |
| `rust-toolchain` | `stable` | Used for CLI compilation and Cargo lockfile work. |
| `uv-version` | setup-uv default | Used when a configured uv lockfile needs preparation. |
| `commit-message` | `chore(release): version packages` | Release commit message. |
| `pr-title` | `chore(release): version packages` | Managed PR title. |

The `/release` Action has no shell-command inputs or CI hooks; the root Action's command inputs do not change this interface.
The caller retains control of project builds, CI, registry publication, and deployment.

## Outputs

Read outputs only after a successful Action step.

- `release-commit`: selected or published verified commit.
  - Empty when no work is selected.
  - `select` skips completed batches unless `retry: 'true'` is set.
- `releases`: compact JSON for every package in the selected batch, including retries:

  ```json
  [{"package":"logice","version":"1.2.3","tag":"v1.2.3"}]
  ```

- `published`: whether this run created any remote tag or GitHub Release.
- `published-packages`: only packages changed remotely during this run.
- `publication-plan`: the inspected remote plan for `select`, `auto`, or `retry-publication`.
- `publication-needed`: whether inspection found incomplete GitHub publication work.
- `ready`: whether inspection or publication completed without conflicts.
  - This is not a CI result or authorization for other publishing systems.
  - PR management alone does not set it to true.
- `has-notes`, `pr-number`, `pr-operation`: PR-management results.

The lower-level `preview` and `publish` Actions also expose `releases`.
The lower-level `publish` Action accepts an optional `expected-release-commit`.
Omission retains its latest-batch behavior.

## External CI and artifacts: Logice example

Use [the Logice-style workflow](../examples/github-actions/logice-release.yml) when external CI must validate the selected commit.
This is a migration example, not a modification to the Logice repository.

Its single package configuration needs no new fields:

```json
{
  "packages": {
    "logice": {
      "path": ".",
      "version_file": "VERSION",
      "version_outputs": ["web/package.json"],
      "tag_prefix": "v"
    }
  }
}
```

The workflow keeps these boundaries:

1. `select` prepares the tools and selects incomplete work with read-only GitHub permissions.
2. The existing reusable CI workflow validates that exact `revision`.
3. `manage-pr` runs independently with PR-write permissions.
4. `publish` receives the CI-selected `expected-release-commit`.
   - If the latest remote batch changes, publication stops before remote writes.
   - It does not silently switch to the newer batch.
5. The existing Docker workflow receives the verified commit and tag from `releases`.
   - The example uses the first entry because Logice has one release package.
   - For multiple packages, select the package explicitly before building artifacts.

The consumer no longer needs a local setup-changenote Action, cache shell commands, internal script paths, or VERSION parsing.
It still needs its own CI and Docker workflows with `workflow_call` inputs `revision` and `version-tag`.
Keep those jobs separate to preserve permission and validation boundaries.

Manual `retry-publication` selects completed batches too.
This reruns CI and downstream artifact publication even if GitHub publication reports `published=false`.
Existing release text and tags are not replaced.

## Verification limits

Local tests cover operation routing, source isolation, setup commands, metadata, and publication guards.
Linux container tests use real Git repositories and a mocked GitHub API.
Hosted cache restoration, tool-install Actions, fork settings, token permissions, and branch rules still need a disposable-repository test.
No hosted publication or registry upload is authorized by these examples.
