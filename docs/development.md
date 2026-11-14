# Development and repository releases

These instructions apply to changenote itself.
For release automation in another repository, use the [GitHub Actions guide](github-actions.md).

- [Repository layout](#repository-layout)
- [Rust checks](#rust-checks)
- [Action checks](#action-checks)
- [Python distribution](#python-distribution)
- [Repository release workflow](#repository-release-workflow)
- [Registry setup](#registry-setup)

## Repository layout

The repository contains one Rust Cargo package and one Python distribution that packages its executable. Neither uses a workspace; the CLI still supports release plans for other monorepos.

- `Cargo.toml`: Rust package metadata, dependencies, and release version source.
- `Cargo.lock`: Rust dependency lockfile.
- `src/`: Rust CLI and release logic.
- `tests/`: Rust integration tests.
- `pyproject.toml`: maturin package metadata and build configuration.
- `uv.lock`: Python project dependency lockfile.
- `python/changenote/`: Python wrapper module for the Rust CLI.
- `python/tests/`: wrapper tests.
- `.changenote/`: repository-level notes and release configuration.
- `release/action.yml`: config-driven Linux GitHub Action with CLI and package-manager setup.
- `action.yml`, `publish/action.yml`, `preview/action.yml`, `check/action.yml`: lower-level Linux GitHub Actions.
- `examples/github-actions/`: minimal and external-CI consumer workflows.
- `scripts/action/`, `tests/actions/`: Action scripts and local Git/mock GitHub tests.

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

## Action checks

For the Actions, install `gh`, `jq`, ShellCheck, actionlint, and zizmor, then run:

```sh
for script in scripts/action/*.sh; do bash -n "$script"; done
shellcheck -x scripts/action/*.sh
# actionlint does not yet parse GitHub's $/ and $/preview references.
# Keep this exact-message filter until upstream support is available.
actionlint -ignore '^specifying action "\$/(preview)?" in invalid format because ref is missing\. available formats are "\{owner\}/\{repo\}@\{ref\}" or "\{owner\}/\{repo\}/\{path\}@\{ref\}"$'
zizmor --offline .github/
cargo build --locked
uv run --no-project python -m unittest discover -s tests/actions -v
uv run --no-project --with pyyaml python -m unittest discover -s tests/action_metadata -v
uv run --no-project python -m unittest discover -s tests/python_release -v
```

Action tests use temporary consumer repositories and a file-backed GitHub mock. They do not create remote GitHub tags or releases.

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

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from root `Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. The crates.io and PyPI jobs validate `Cargo.toml` against the saved release batch and check the local package version in `Cargo.lock`. Older releases configured with `VERSION` receive build-only Cargo version synchronization in a disposable checkout. The wheel check uses the validated version, not a separate version file. For manual publication, update the Cargo version and lockfile first.

CI currently runs on Ubuntu only. It runs Rust format checks, lint checks, and tests, plus Python wrapper tests, binary wheel builds, and uvx checks with Python 3.14. The container image uses the Rust executable; mount a repository at `/repo` to use it.

## Repository release workflow

Production workflows pin external Actions to full commit SHAs.
Dependabot uses a seven-day cooldown for each configured ecosystem.
Release PR and publication jobs build the CLI from the workflow commit and use GitHub's `$/` self-repository references for Action code from that same commit.
Publication still selects the release from the remote base branch.

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

The workflow builds the CLI instead of installing it and does not push version commits directly to `main`. It publishes the Rust package from the validated release commit to crates.io with `cargo publish --locked` and Trusted Publishing. The `rust-lang/crates-io-auth-action` Action uses GitHub OIDC to obtain a short-lived token. Only the crates.io and PyPI upload jobs have `id-token: write`. The crates.io upload step receives its short-lived token through the `CARGO_REGISTRY_TOKEN` environment variable. No crates.io token secret is required. The workflow also builds Python distributions from that commit and uploads them to PyPI with `uv publish --trusted-publishing always` and GitHub OIDC, without a PyPI token secret. See the [Python distribution guide](../python/README.md) for supported platforms and required PyPI settings. This routing policy is repository-specific.

## Registry setup

### crates.io

Configure crates.io publication before the first upload:

1. Create the GitHub environment `crates-io`. Restrict deployments to `main`; add required reviewers if approval is needed.
2. Configure the `changenote` crate's [Trusted Publisher](https://crates.io/docs/trusted-publishing) for the GitHub repository `narumiruna/changenote`.
   - Set the workflow filename to `release.yml`.
   - Set the environment to `crates-io`.

After the first crates.io publication, users can install the CLI with `cargo install changenote --locked`.

### PyPI

The existing [changenote PyPI project](https://pypi.org/project/changenote/) is the publication target. The Release workflow builds platform wheels with maturin Action and publishes with `uv publish` and Trusted Publishing. Before enabling uploads, the PyPI project owner must configure a GitHub Trusted Publisher for owner `narumiruna`, repository `changenote`, workflow filename `release.yml`, and environment `pypi`. Uploads fail if this trust is absent or does not match. After a successful publication, users on supported platforms can run `uvx changenote status`. See the [Python distribution guide](../python/README.md).
