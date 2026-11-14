# Development and repository releases

These instructions apply to changenote itself.
For release automation in another repository, use the [GitHub Actions guide](https://github.com/changenote/action/blob/main/docs/github-actions.md).
Action implementation, consumer examples, and integration tests are maintained in [changenote/action](https://github.com/changenote/action).

- [Repository layout](#repository-layout)
- [Rust checks](#rust-checks)
- [Workflow and publication checks](#workflow-and-publication-checks)
- [Python distribution](#python-distribution)
- [Repository release workflow](#repository-release-workflow)
- [Registry setup](#registry-setup)

## Repository layout

The repository uses one Cargo workspace with one member, `crates/changenote`. The Python distribution packages its executable and does not use a uv workspace. The CLI supports release plans for other monorepos.

- `Cargo.toml`: virtual Cargo workspace with resolver `3`.
- `Cargo.lock`: shared Rust dependency lockfile.
- `crates/changenote/Cargo.toml`: Rust package metadata, explicit version, and dependencies.
- `crates/changenote/src/`: Rust CLI and release logic.
- `crates/changenote/tests/`: Rust integration tests and fixtures.
- `pyproject.toml`: maturin package metadata and build configuration.
- `uv.lock`: Python project dependency lockfile.
- `python/changenote/`: Python wrapper module for the Rust CLI.
- `python/tests/`: wrapper tests.
- `.changenote/`: repository-level notes and release configuration.
- `.github/workflows/`: CLI checks, release routing, and CLI registry publication.
- `scripts/route-release.py`: repository-specific release routing and preview checks.
- `scripts/publish-crate.py`: repository-specific crates.io publication helper.
- `scripts/prepare-python-release.py`: validated release version preparation for crates and wheels.
- `crates/changenote/SKILL.md`: the single source for the bundled agent skill.
- `tests/workflows/`, `tests/publication/`, `tests/python_release/`: CLI workflow and publication tests.

## Rust checks

Run checks from the repository root:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
cargo run --locked -p changenote -- status
```

The Rust package and executable names are `changenote`, so `cargo run --locked -p changenote -- status` works from the repository root. Rust build outputs go to root `target/`. The member has no separate lockfile or inherited workspace version.

To install from this checkout, run `cargo install --locked --path crates/changenote`.
Registry installation remains `cargo install changenote --locked`.

### Package resources

The agent skill has one source, `crates/changenote/SKILL.md`. Edit that file directly. Agents can run `changenote skill` to read the embedded Markdown without a source checkout. There is no root skill copy or synchronization script.

Root `README.md`, `LICENSE`, `schema.json`, `docs/notes.md`, and `docs/swift.md` remain canonical for the other package resources. After changing one of those files, copy the current resources into the crate:

```sh
cp README.md LICENSE schema.json crates/changenote/
cp docs/notes.md docs/swift.md crates/changenote/docs/
```

Include the updated copies in the same commit. Publication tests compare their bytes and reject missing or stale copies. Documentation links use absolute repository URLs when the target guide is not shipped in the crate. The crate includes these resources and the hidden `.changenote` planning fixtures, so extracted Cargo and Python source archives need no repository resource paths or runtime downloads. CI packages the crate and runs its Rust tests from an extracted archive outside the checkout.
The exact changelog fixtures keep their final blank-line separator; `.gitattributes` and the newline hook preserve these bytes.

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
uv build --out-dir dist/python-binary
uvx --isolated --no-index --find-links dist/python-binary changenote --help
```

The Python distribution `changenote`, with wrapper sources in `python/changenote`, packages the Rust executable using maturin `bin` bindings, like uv. It does not reimplement the CLI in Python. Compatible wheels require no Rust toolchain at runtime; source builds require Rust and Cargo. The optional `python -m changenote` runner invokes the installed executable.

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from `crates/changenote/Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. The `.changenote` package path stays `.` with version file `crates/changenote/Cargo.toml`; this keeps the root changelog and repository-wide note coverage. The crates.io and PyPI jobs validate that manifest against the saved release batch and check the local package version in root `Cargo.lock`. Historical releases with a root Cargo package remain supported. Older releases configured with `VERSION` receive build-only Cargo version synchronization in a disposable checkout. The wheel check uses the validated version, not a separate version file. For manual publication, update the Cargo version and lockfile first.

CI checks run on Ubuntu; release wheel builds also use macOS and Windows. CI runs Rust format checks, lint checks, and tests, plus Python wrapper tests, binary wheel builds, and uvx checks with Python 3.14.

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

The repository's `.github/workflows/release.yml` only manages the Release PR on ordinary pushes to `main`, even when an older release batch exists. Automatic publication requires a new batch-changing first-parent commit in the push range and exactly one associated merged managed PR. The PR must use the reserved head branch (`changenote-release/main`, or the previous `narumi/chore/changenote-release-main` name for older pinned Actions), the repository's `main` base, and the managed body marker. Its merge commit SHA must match the batch commit. PR titles and commit messages do not authorize publication. Force pushes, missing history, invalid batches, ambiguous associations, and API errors stop routing without authorizing publication.

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
