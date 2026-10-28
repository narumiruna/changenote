# Config-driven GitHub releases

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

There are no shell-command inputs or CI hooks.
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
