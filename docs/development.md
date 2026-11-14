# Development and repository releases

These instructions apply to changenote itself.
For release automation in another repository, use the [GitHub Actions guide](https://github.com/changenote/action/blob/main/docs/github-actions.md).
Action implementation, consumer examples, and integration tests are maintained in [changenote/action](https://github.com/changenote/action).

- [Repository layout](#repository-layout)
- [Rust checks](#rust-checks)
- [Workflow and publication checks](#workflow-and-publication-checks)
- [Python distribution](#python-distribution)
- [Workspace migration verification](#workspace-migration-verification)
- [Repository release workflow](#repository-release-workflow)
- [Registry setup](#registry-setup)

## Repository layout

The repository uses a Cargo workspace with one Rust package and one Python distribution that packages its executable. Python does not use a uv workspace.

- `Cargo.toml`: virtual workspace with `crates/changenote` as its only member and default member.
- `Cargo.lock`: shared Rust dependency lockfile.
- `crates/changenote/Cargo.toml`: Rust package metadata, dependencies, and explicit release version.
- `crates/changenote/src/`: Rust CLI and release logic.
- `crates/changenote/tests/`: Rust integration tests and planning fixtures.
  - Golden `expected-*-CHANGELOG.md` fixtures retain the generated trailing blank line. The end-of-file hook excludes only these byte-exact files; tests verify their bytes and the exclusion scope.
- `skills/changenote/SKILL.md` and root `LICENSE`: canonical public skill and license.
  - Keep `crates/changenote/assets/changenote.SKILL.md` and `crates/changenote/LICENSE` byte-identical to these files. Publication tests reject different copies.
  - The package-local copies let Cargo and Python source archives build without the surrounding repository.
- `pyproject.toml`: maturin package metadata and build configuration.
- `uv.lock`: Python project dependency lockfile.
- `python/changenote/`: Python wrapper module for the Rust CLI.
- `python/tests/`: wrapper tests.
- `.changenote/`: repository-level notes and release configuration.
- `.github/workflows/`: CLI checks, release routing, and CLI registry publication.
- `scripts/route-release.py`: repository-specific release routing and preview checks.
- `scripts/publish-crate.py`: repository-specific crates.io publication helper.
- `scripts/prepare-python-release.py`: validated release version preparation for crates and wheels.
- `tests/workflows/`, `tests/publication/`, `tests/python_release/`: CLI workflow, metadata, resource, and publication tests.
- `tests/distribution/`: standalone Cargo and Python source archive build/install tests.

## Rust checks

Run checks from the repository root:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
cargo run --locked -p changenote -- status
```

The Rust package and executable names are `changenote`, so `cargo run --locked -p changenote -- status` works from the repository root. Rust build outputs go to `target/`.

## Workflow and publication checks

Install actionlint, zizmor, and uv, then run from the CLI repository root:

```sh
actionlint
zizmor --offline .github/
uv run --no-project --with pyyaml python -m unittest discover -s tests/workflows -v
uv run --no-project python -m unittest discover -s tests/publication -v
uv run --no-project python -m unittest discover -s tests/python_release -v
```

These tests validate CLI workflow metadata, routing, and publication helpers without remote uploads.
For the shared Actions' shell checks and Git/mock GitHub integration tests, use the [Action development guide](https://github.com/changenote/action/blob/main/docs/development.md).

## Python distribution

For the Python distribution, run from the repository root:

```sh
uv sync --locked
uv run --locked changenote status
uv run --locked python -m changenote --help
uv run --locked python -m unittest discover -s python/tests
uv run --no-project python -m unittest discover -s tests/distribution -v
uv build --out-dir dist/python-binary
uvx --isolated --no-index --find-links dist/python-binary changenote --help
```

The Python distribution `changenote`, with wrapper sources in `python/changenote`, packages the Rust executable using maturin `bin` bindings, like uv. It does not reimplement the CLI in Python. Compatible wheels require no Rust toolchain at runtime; source builds require Rust and Cargo. The optional `python -m changenote` runner invokes the installed executable.

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from `crates/changenote/Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. The release package keeps `path: "."` and uses `version_file: "crates/changenote/Cargo.toml"`; this preserves root changelog output and note coverage for Rust, Python, and build files. The crates.io and PyPI jobs validate the configured Cargo manifest against the saved release batch and check the local package version in root `Cargo.lock`. Preparation also supports historical root `Cargo.toml` releases. Older releases configured with root `VERSION` receive build-only root Cargo version synchronization in a disposable checkout. The wheel check uses the validated version, not a separate version file. For manual publication, update the Cargo version and lockfile first.

CI currently runs on Ubuntu only. It runs Rust format checks, lint checks, and tests, plus Python wrapper tests, standalone source archive builds and installations, binary wheel builds, and uvx checks with Python 3.14.

## Workspace migration verification

The single-member workspace migration was checked on Linux x86_64 on 2026-11-14.

- All 187 Rust tests passed, with format, Clippy, release-build, and verified Cargo package checks.
- Workflow tests (29), publication tests (10), release-preparation tests (6), Python wrapper tests (11), and standalone installation tests (3) passed. actionlint, offline zizmor, and changed-file commit hooks passed.
- The optional `prek run --all-files` check found pre-existing whitespace and final-newline issues in root `CHANGELOG.md`. Its original bytes were restored. This migration does not change that release file.
- A `.crate` was extracted and installed outside the checkout. A temporary Git repository verified `cargo install --git` discovery of the nested member. A Python sdist built a wheel in a separate temporary directory; the installed executable and Python runner passed version, skill, help, completion, and wrapper checks.
- CLI version, help, skill bytes, and all three completion scripts matched the pre-migration outputs. Existing Cargo targets, library exports, package/dependency versions, root lockfile, saved batch, and changelog were preserved. The pending patch note is the only new release request.
- Current nested Cargo, historical root Cargo, and historical root VERSION preparation passed. Failure cases cover invalid source paths, versions, batch/package mismatches, and invalid lockfile identity without writes.

These checks do not establish native macOS, Windows, Linux aarch64, or manylinux 2.17 compatibility, hosted authentication, or successful registry publication.

1. Before release of the workspace migration, the repository maintainer must arrange separately authorized native wheel build/install checks after the local checks pass.
   - Check the remaining supported platforms and manylinux targets with build-only jobs or native machines. Expect the same package version and embedded skill.
   - Keep release approval blocked until the required platform evidence exists. Do not dispatch the Release workflow only to test this migration.
2. After implementation review and the required local and platform checks, the release maintainer must obtain separate publication authorization.
   - Keep package names, tag format, Action pins, permissions, and Trusted Publisher settings unchanged.
   - Local checks do not authorize uploads, tags, or a Release workflow dispatch.

## Repository release workflow

Production workflows pin external Actions to full commit SHAs.
Dependabot uses a seven-day cooldown for each configured ecosystem.
Release PR and publication jobs test and build the CLI from the workflow commit, then call shared Actions from `changenote/action`.
The migration pins Action revision `4b2d07126b8a4da4b44aa5fb8256dd81a69e2e3a` from [Action PR #1](https://github.com/changenote/action/pull/1).
They no longer use `$/` self-repository references or local `scripts/action/` helpers.
Publication still selects the release from the remote base branch.
Merge the Action PR before the CLI migration PR removes the old entry points from remote main.
The CLI and Action have independent release versions and commit SHAs.
The Action pins a compatible CLI revision; a CLI release does not automatically update that pin.
Do not use an unpinned branch or reuse the old repository SHA with the new Action path.
See the [migration procedure](https://github.com/changenote/action/blob/main/docs/migration.md).

### Routing and verification

The repository's `.github/workflows/release.yml` only manages the Release PR on ordinary pushes to `main`, even when an older release batch exists. Automatic publication requires a new batch-changing first-parent commit in the push range and exactly one associated merged managed PR. The PR must use the reserved head branch, the repository's `main` base, and the managed body marker. Its merge commit SHA must match the batch commit. PR titles and commit messages do not authorize publication. Force pushes, missing history, invalid batches, ambiguous associations, and API errors stop routing without authorizing publication.

Before publication, the workflow previews and validates the latest remote batch. Automatic runs require that batch to match the routed commit; publication checks the selection again after its own fetch and before any remote writes. If a newer release has replaced it, the run fails and requires an explicit retry. Multiple releases in one push still select only the latest batch. An unrelated later commit does not change the selected batch.

Routing runs for every event, outside concurrency groups.
PR management and publication use separate job concurrency groups.
A later ordinary push can replace a pending PR update, but cannot replace a pending publication.
Publication does not depend on the PR-management job.
Keep required publication steps in this workflow: tags pushed with `GITHUB_TOKEN` do not trigger other push workflows.

### Retry a partial publication

Manual `operation=retry-publication` skips PR management and retries the latest validated batch. crates.io publication and wheel builds run after successful GitHub publication with a non-empty validated commit, even if `published=false` because the GitHub tag and Release already exist. A missing batch is a no-op. Cancellation or failed GitHub publication prevents registry uploads. crates.io publication and PyPI builds run independently, so a failure in one does not block the other.

The default manual operation, `manage-pr`, only manages the Release PR.
Run **Release** manually on `main` with `operation=retry-publication` to retry a partial failure.
Do not merge another Release PR until the current batch is fully published: discovery covers only the latest batch.
The Action uses the saved release commit even after unrelated commits.
History rewrites or manual batch edits require manual inspection.
Never move tags automatically.

The crates.io job checks the exact `changenote` version before upload. An existing non-yanked version is skipped; its contents are not compared with the release checkout. A yanked version, invalid response, or API error stops the job. Only HTTP 404 permits a new upload. Retry builds can differ from files already on PyPI: `uv publish --check-url` skips matching files but rejects different contents for the same filename. Inspect registry state and resolve conflicts separately; never replace published files automatically.

### Registry publication

The CLI workflow builds the CLI instead of installing it and does not push version commits directly to `main`.
Registry publication stays in this repository. The crates.io job uses `scripts/publish-crate.py`; it is not a shared Action helper.
The Action repository's `/release` entry point instead installs its separate trusted CLI pin.

The CLI workflow publishes the Rust package from the validated release commit to crates.io with `cargo publish --locked` and Trusted Publishing. The `rust-lang/crates-io-auth-action` Action uses GitHub OIDC to obtain a short-lived token. Only the crates.io and PyPI upload jobs have `id-token: write`. The crates.io upload step receives its short-lived token through the `CARGO_REGISTRY_TOKEN` environment variable. No crates.io token secret is required. The workflow also builds Python distributions from that commit and uploads them to PyPI with `uv publish --trusted-publishing always` and GitHub OIDC, without a PyPI token secret. See the [Python distribution guide](../python/README.md) for supported platforms and required PyPI settings. This routing policy is repository-specific.

## Source-link rollout

Local source-link tests use disposable Git histories and a locally built wheel installed in an isolated test environment.
Action contract tests use local remotes and a mocked GitHub API, not production publication.
These checks do not verify hosted event delivery, permissions, branch rules, registry access, or end-to-end publication.
They do not measure agent task success for the updated skill.

1. The CLI release maintainer must publish compatible artifacts, including PyPI wheels, after a separate release authorization.
   - The expected result is an installable CLI that accepts `changelog.source_repository`.
   - Do not enable the setting in production before those artifacts exist.
2. The repository maintainer must select the same exact compatible CLI version for generation, preview, and publication before an authorized rollout.
   - If Action implementation changes are needed, review its published commit SHA separately.
   - Test hosted event delivery, permissions, branch rules, and publication in an authorized disposable repository before production use.
   - Local validation does not authorize a production configuration change, workflow dispatch, or publication.

To downgrade tooling before generating a batch, first remove `changelog.source_repository`; older CLIs reject the field.
For a committed batch, retain compatible tooling and its historical configuration until publication completes.
Do not remove links from its generated tree, bypass replay, edit saved batches, move tags, or rewrite published Releases.

## Registry setup

### crates.io

Configure crates.io publication before the first upload:

1. Create the GitHub environment `crates-io`. Restrict deployments to `main`; add required reviewers if approval is needed.
2. Configure the `changenote` crate's [Trusted Publisher](https://crates.io/docs/trusted-publishing) for the GitHub repository `changenote/changenote`.
   - Set the workflow filename to `release.yml`.
   - Set the environment to `crates-io`.

After the first crates.io publication, users can install the CLI with `cargo install changenote --locked`.

### PyPI

The existing [changenote PyPI project](https://pypi.org/project/changenote/) is the publication target. The Release workflow builds platform wheels with maturin Action and publishes with `uv publish` and Trusted Publishing. Before enabling uploads, the PyPI project owner must configure a GitHub Trusted Publisher for owner `changenote`, repository `changenote`, workflow filename `release.yml`, and environment `pypi`. Uploads fail if this trust is absent or does not match. After a successful publication, users on supported platforms can run `uvx changenote status`. See the [Python distribution guide](../python/README.md).
