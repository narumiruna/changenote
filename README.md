# changenote

A Rust CLI for language-independent change notes, SemVer versions, changelogs, and release tags. Inspired by [Changesets](https://github.com/changesets/changesets).

Each package has an explicit directory and a plain-text version file. No language runtime or package manager is required after the CLI is built.

## Install

Requires stable Rust with edition 2024 support. Git is required only for `tag`.

```sh
cargo install --path . --locked
```

## Single package

Run commands from the repository root, or use the global `--root PATH` option.

```sh
changenote init --name pkg-a --version 0.0.0
changenote add hello-world --package pkg-a --bump minor --message "Initial release"
```

This creates `.changenote/hello-world.md`:

```markdown
---
pkg-a: minor
---

Initial release
```

Quoted keys, such as `"pkg-a": minor`, are also accepted. You can write notes directly without the CLI.

```sh
changenote status   # JSON plan; no file changes
changenote version  # Apply plan and consume notes

git add -- .changenote VERSION CHANGELOG.md
git commit -m "chore(release): version packages"
changenote tag      # Create local annotated tag v0.1.0

git push origin HEAD
git push origin refs/tags/v0.1.0
```

`version` takes the highest requested bump for each package and applies it once. Multiple patch notes cause one patch bump, not multiple bumps. Major, minor, and patch follow stable SemVer rules, including versions below 1.0.0.

The command updates `VERSION`, prepends entries to `CHANGELOG.md`, saves the latest release batch to `.changenote/releases.json`, and removes consumed notes. A run without pending notes does not change files or clear the saved batch.

## Monorepo

Create `.changenote/config.json` with explicit packages:

```json
{
  "packages": {
    "pkg-a": { "path": "packages/pkg-a", "version_file": "VERSION" },
    "pkg-b": { "path": "packages/pkg-b", "version_file": "VERSION" }
  }
}
```

Each package directory must exist and contain its version file, for example `packages/pkg-a/VERSION`:

```text
0.1.0
```

One note can name multiple packages with different bumps:

```markdown
---
"pkg-a": patch
"pkg-b": minor
---

Update the shared API
```

With two or more configured packages, tags use `changenote/<package>@v<version>`. A patch for `pkg-a` at `0.1.0` creates `changenote/pkg-a@v0.1.1`. Packages without notes remain unchanged. Tag format depends on the number of configured packages, not the number changed in a release.

To create a note with the same bump for several packages:

```sh
changenote add shared-api -p pkg-a -p pkg-b --bump minor -m "Update the shared API"
```

## GitHub Actions

This repository provides two reusable composite Actions:

- `narumiruna/changenote@<full-commit-sha>`: create or update a Release PR.
- `narumiruna/changenote/publish@<full-commit-sha>`: publish annotated tags and GitHub Releases.

Both require Linux, Bash, Git, `gh`, `jq`, a clean checkout, and an installed `changenote` executable on `PATH`. Ubuntu GitHub-hosted runners include the non-CLI tools. Package version files and changelogs must not be symlinks. Run at the repository root; the Actions load their scripts from the Action checkout, not the consumer repository. They do not install the CLI, publish registries, or synchronize package manifests.

### Release PR workflow

Use this workflow in a consumer repository. Replace each SHA placeholder with a reviewed full commit SHA. The CLI and Action must use compatible release-plan formats.

```yaml
name: Release PR
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
concurrency:
  group: changenote-release-main
  cancel-in-progress: false
jobs:
  release:
    permissions:
      contents: write
      pull-requests: write
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote@<full-action-commit-sha>
        with:
          base-branch: main

  publish:
    needs: release
    if: ${{ always() && github.ref == 'refs/heads/main' && needs.release.result != 'cancelled' }}
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote/publish@<full-action-commit-sha>
        with:
          base-branch: main
```

The Action fetches the current base branch, applies pending notes in a temporary worktree, and creates or updates one PR from `narumi/chore/changenote-release-<base-branch>`. Reserve that branch for automation. New notes update the same PR; unchanged generated trees do not create new commits or pushes. If notes are removed, the Action closes only its managed PR and leaves the branch in place. A manually closed PR is replaced when notes remain.

Existing branches require a marked PR in their history. Lease-based pushes stop if another actor moves the branch. The Action stages only release outputs and consumed notes. Notes and titles are data, not shell commands. PR bodies over 60,000 bytes omit release details.

In repository settings, enable **Actions > General > Allow GitHub Actions to create and approve pull requests**. Branch rules must permit the bot to push the release branch. The Action never approves or merges its PR.

PRs created with the default `GITHUB_TOKEN` do not automatically trigger PR workflows. If CI must run on the Release PR, use a GitHub App token (recommended) or PAT with repository contents and pull requests write permissions. Pass it through `github-token`. Checkout credentials do not select the Action's token.

### Publication workflow

The automatic workflow above runs publication after PR management. Publication can recover an earlier batch even if PR management fails. Its job needs only `contents: write`.

For a separate publication retry workflow, use **only** `workflow_dispatch`, the same concurrency group, and the publication job's checkout and install steps. Its final step is:

```yaml
- uses: narumiruna/changenote/publish@<full-action-commit-sha>
  with:
    base-branch: main
```

Use the **same concurrency group** for automatic and manual workflows. Do not create two push-triggered workflows in that group: GitHub can cancel a pending run even with `cancel-in-progress: false`. Keep required publication steps in the automatic workflow: tags pushed with `GITHUB_TOKEN` do not trigger other push workflows.

Publication selects the latest first-parent commit that changes `.changenote/releases.json`. It reconstructs that commit from the parent's notes and compares the complete generated tree before publication. Standard merge, squash, and rebase merges are supported. A later unrelated commit or new pending notes do not change the release target.

Existing remote tags must point to the selected commit. Missing tags and GitHub Releases are created; existing release text is not replaced. Draft releases, missing tags for existing releases, malformed batches, batch deletion, manually edited release trees, and tag conflicts stop publication. A batch in a root commit cannot be verified. No batch is a no-op.

The repository's `.github/workflows/release-pr.yml` runs both jobs on pushes to `main` or manual dispatch. `.github/workflows/release.yml` is a publication-only manual retry. They build the CLI instead of installing it. Neither pushes version commits directly to `main`.

### Inputs and outputs

| Action | Input | Default |
| --- | --- | --- |
| Both | `github-token` | Workflow token |
| Both | `base-branch` | Repository default branch |
| Release PR | `commit-message` | `chore(release): version packages` |
| Release PR | `pr-title` | `chore(release): version packages` |

The PR Action outputs `has-notes` (`true` or `false`), `pr-number` (number or empty string), and `pr-operation` (`created`, `updated`, `closed`, or `none`). `updated` includes a no-op version rerun that refreshes the PR metadata.

The publication Action outputs `release-commit` (commit SHA or empty string), `published` (`true` if this run created any remote tag or GitHub Release), and `published-packages` (JSON, for example `[{"name":"pkg-a","version":"1.2.0"}]`). This is not registry publication. Read outputs only after successful steps.

### Recovery and limits

Disable both workflows to stop automation. Unmerged Release PR changes do not affect the base branch.

After a partial publication failure, rerun `Release`. The Action uses the saved release commit even after unrelated commits. Do not merge another Release PR until the current batch is fully published: discovery covers only the latest batch. History rewrites or manual batch edits require manual inspection. Never move tags automatically.

If PR creation fails after pushing a new branch, the Action checks for a successful remote PR creation before it removes that branch with a lease. If the API or cleanup also fails, inspect the marked PR and branch manually before retrying; an unowned branch is deliberately rejected. Do not delete a branch without checking its commit and PR history.

Before production use, verify repository permissions, branch rules, and App-token CI behavior in a disposable repository.

## Scope and safety

- Version files contain only a stable SemVer string. They are the release version source; `Cargo.toml`, `package.json`, `pyproject.toml`, lockfiles, and internal dependencies are **not** updated. Use a package-specific build or publication step if those files require synchronization. The CLI's `--version` reports its Cargo build version, not a configured package version.
- Names use ASCII letters, digits, hyphens, and underscores. Paths are relative to the repository root and cannot contain parent components. Package output files cannot overlap or resolve outside the repository.
- Unknown packages, invalid bumps, duplicate YAML keys, empty summaries, and prerelease versions are rejected. `add` never overwrites a note.
- Only top-level `.changenote/*.md` files are read. Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
- `tag` requires a clean Git working tree and committed release data. It creates local tags only; it does not push, publish artifacts, or run arbitrary commands.
- Input validation and output preparation happen before version writes. Filesystem failures during writes can leave partial changes. Run in a clean checkout, inspect changes, and use Git to restore files before retrying. Do not run concurrent local version commands.
- Dependency propagation, registry publication, prereleases, and fixed or linked version groups are not implemented.

## Development

The repository contains one Rust Cargo package and one Python distribution that packages its executable. Neither uses a workspace; the CLI still supports release plans for other monorepos.

- `Cargo.toml`: Rust package metadata and dependencies.
- `Cargo.lock`: Rust dependency lockfile.
- `src/`: Rust CLI and release logic.
- `tests/`: Rust integration tests.
- `VERSION`: CLI release-plan version file.
- `pyproject.toml`: maturin package metadata and build configuration.
- `uv.lock`: Python project dependency lockfile.
- `python/changenote/`: Python wrapper module for the Rust CLI.
- `python/tests/`: wrapper tests.
- `.changenote/`: repository-level notes and release configuration.
- `action.yml`, `publish/action.yml`: reusable Linux GitHub Actions.
- `scripts/action/`, `tests/actions/`: Action scripts and local Git/mock GitHub tests.

Run checks from the repository root:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
cargo run --locked -p changenote -- status
```

The Rust package and executable names are `changenote`, so `cargo run -- status` works from the repository root. Rust build outputs go to `target/`.

For the Actions, install `gh`, `jq`, ShellCheck, and actionlint, then run:

```sh
for script in scripts/action/*.sh; do bash -n "$script"; done
shellcheck -x scripts/action/*.sh
actionlint
cargo build --locked
uv run --no-project python -m unittest discover -s tests/actions -v
uv run --no-project --with pyyaml python -m unittest discover -s tests/action_metadata -v
```

Action tests use temporary consumer repositories and a file-backed GitHub mock. They do not create remote GitHub tags or releases.

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

The existing [changenote PyPI project](https://pypi.org/project/changenote/) is the publication target. After publishing this binary-wrapper version and platform wheels, users can run `uvx changenote status`. This wrapper has not yet been uploaded, and a production multi-platform wheel release pipeline is not configured. See `python/README.md`.

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from root `Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. `VERSION` is not synchronized with Cargo automatically; align the Rust crate version with that release-plan version and update lockfiles before publication.

CI runs Rust format checks, lint checks, and tests on Linux, macOS, and Windows, plus Python wrapper tests, binary wheel builds, and uvx checks on Python 3.12 and 3.14 on those platforms. The container image uses the Rust executable; mount a repository at `/repo` to use it.
