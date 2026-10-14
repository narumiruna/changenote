# changenote

A Rust CLI for language-independent change notes, SemVer versions, changelogs, and release tags. Inspired by [Changesets](https://github.com/changesets/changesets).

Each package has an explicit directory and a version file: a package manifest or a plain-text file. No language runtime or package manager is required after the CLI is built.

## Install

Requires stable Rust with edition 2024 support. Git is required for `check` and `tag`.

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

The command updates each configured version file, adds entries to `CHANGELOG.md`, saves the latest release batch to `.changenote/releases.json`, and removes consumed notes. New entries follow the existing title and introduction, before the first ATX SemVer version heading outside fenced or indented code. A file without a version heading is treated as an introduction. Prior content is not reformatted. Notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`, in that order; empty groups are omitted. Multiline Markdown remains inside each note's list item. A run without pending notes does not change files or clear the saved batch.

## Monorepo

Create `.changenote/config.json` with explicit packages:

```json
{
  "$schema": "https://raw.githubusercontent.com/narumiruna/changenote/main/schema.json",
  "packages": {
    "pkg-a": { "path": "packages/pkg-a", "version_file": "VERSION" },
    "pkg-b": { "path": "packages/pkg-b", "version_file": "VERSION" }
  }
}
```

The optional `$schema` field enables editor validation and completion with the repository's [JSON Schema](schema.json). `init` adds the GitHub raw URL shown above. Replace `main` with a commit SHA to pin the schema, or use a local schema path. The CLI accepts the field as metadata; it does not fetch the schema or use it for validation. Configurations without `$schema` remain supported. Unknown configuration fields are rejected. The CLI also checks paths, files, and versions that the schema cannot verify.

Each package directory must exist and contain its version file, for example `packages/pkg-a/VERSION`:

```text
0.1.0
```

Packages in any language can use a plain-text `VERSION` file, including Python, Rust, Node.js, and Go. Package manifests are optional version sources. You can mix manifests and `VERSION` files in the same repository:

```json
{
  "packages": {
    "pkg-python": { "path": "packages/pkg-python", "version_file": "pyproject.toml" },
    "pkg-rust": { "path": "packages/pkg-rust", "version_file": "Cargo.toml" },
    "pkg-node": { "path": "packages/pkg-node", "version_file": "package.json" },
    "pkg-go": { "path": "packages/pkg-go", "version_file": "VERSION" }
  }
}
```

`version_file` is relative to the package directory. Its filename selects the version field:

| Filename | Version field |
| --- | --- |
| `pyproject.toml` | `project.version` |
| `Cargo.toml` | `package.version` |
| `package.json` | Top-level `version` |
| Other filenames, including `VERSION` | Plain-text stable SemVer string |

Manifest versions must be explicit strings. Dynamic Python versions, Poetry-only versions, and inherited Cargo workspace versions are not supported. TOML comments and unrelated fields are preserved. JSON updates replace only the top-level version value token. Every other byte is preserved, including indentation, line endings, field order, number spelling, and the presence or absence of a final newline. Duplicate top-level version keys, including escaped spellings of the same key, are rejected. Lockfiles and dependency versions are not updated. `init` still creates a plain-text `VERSION`; configure existing manifests manually.

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

To use different bumps in one note:

```sh
changenote add shared-api --minor pkg-a --patch pkg-b -m "Update the shared API"
```

Repeat `--major`, `--minor`, or `--patch` for each package. Do not combine these options with `--package` or `--bump`. Conflicting bumps for one package and unknown packages fail before a file is created. Repeated identical assignments are deduplicated. The original CLI syntax and Rust `add()` API remain supported; Rust callers can use `add_with_bumps()` for mixed bumps.

## Check PR notes

```sh
changenote check --since origin/main
changenote check --since origin/main --skip-release --reason "Documentation only"
```

`check` requires the Git repository root, a clean working tree, full enough history to find a merge base, and an available base ref. It compares tracked paths between `git merge-base <ref> HEAD` and `HEAD`; it does not include uncommitted or untracked changes. File renames report both old and new paths. Nested packages use the deepest matching configured directory. Files outside all configured packages are reported but do not imply a package release.

If packages changed, at least one valid top-level note must be added, modified, or renamed in that comparison and remain in `HEAD`. Unchanged old notes and deleted notes do not satisfy the check. This is a **PR-level check, not proof of per-package note coverage**. No file patterns are excluded automatically. Use the explicit exemption for documentation-only or test-only changes inside package directories. `--skip-release` requires a non-empty `--reason`; it never bypasses config, version, note, or Git validation.

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

## GitHub Actions

This repository provides two reusable composite Actions:

- `narumiruna/changenote@<full-commit-sha>`: create or update a Release PR.
- `narumiruna/changenote/publish@<full-commit-sha>`: publish annotated tags and GitHub Releases.

Both require Linux, Bash, Git, `gh`, `jq`, a clean checkout, and an installed `changenote` executable on `PATH`. Ubuntu GitHub-hosted runners include the non-CLI tools. Package version files, changelogs, and `.changenote/releases.json` must not be symlinks. Both Actions check the batch path before CLI writes. Run at the repository root; the Actions load their scripts from the Action checkout, not the consumer repository. They do not install the CLI or publish registries. The CLI updates configured version files only; it does not synchronize other manifests or lockfiles.

### Release PR workflow

Use this workflow in a consumer repository. Replace each SHA placeholder with a reviewed full commit SHA. The CLI and Action must use compatible release-plan formats.

```yaml
name: Release
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

Release PR descriptions use a `Releases` heading and `package@version` headings. Notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`; empty groups are omitted. Multiline notes remain within their list items. The Action requires a CLI that supports `status --include-note-bumps`. This option adds a `note_bumps` array aligned with `notes`; default release-plan output and saved batches are unchanged.

Existing branches require a marked PR in their history. Lease-based pushes stop if another actor moves the branch. The Action stages only release outputs and consumed notes. Notes and titles are data, not shell commands. PR bodies over 60,000 bytes omit release details.

In repository settings, enable **Actions > General > Allow GitHub Actions to create and approve pull requests**. Branch rules must permit the bot to push the release branch. The Action never approves or merges its PR.

PRs created with the default `GITHUB_TOKEN` do not automatically trigger PR workflows. If CI must run on the Release PR, use a GitHub App token (recommended) or PAT with repository contents and pull requests write permissions. Pass it through `github-token`. Checkout credentials do not select the Action's token.

### Publication workflow

The automatic workflow above runs publication after PR management. Publication can recover an earlier batch even if PR management fails. Its job needs only `contents: write`.

To retry a partial publication failure, run **Release** manually on `main`. This runs both PR management and publication again. They use one workflow concurrency group, so no separate retry workflow is needed. Keep required publication steps in this workflow: tags pushed with `GITHUB_TOKEN` do not trigger other push workflows.

Publication selects the latest first-parent commit that changes `.changenote/releases.json`. It reconstructs that commit from the parent's notes and compares the complete generated tree before publication. The publication Action first checks current output, then the exact legacy output (flat changelogs prepended before titles and two-space JSON with a final newline). Mixed output profiles and arbitrary edits are rejected. The hidden `version --legacy-output` option is reserved for this reconstruction in a temporary Action worktree; do not use it for new releases. Upgrade CLI and Action pins together. Older Actions do not support this fallback. Standard merge, squash, and rebase merges are supported. A later unrelated commit or new pending notes do not change the release target.

Existing remote tags must point to the selected commit. Missing tags and GitHub Releases are created; existing release text is not replaced. Draft releases, missing tags for existing releases, malformed batches, batch deletion, manually edited release trees, and tag conflicts stop publication. A batch in a root commit cannot be verified. No batch is a no-op.

The repository's `.github/workflows/release.yml` runs PR management and GitHub publication on pushes to `main` or manual dispatch. It builds the CLI instead of installing it and does not push version commits directly to `main`. It then builds Python distributions from the validated release commit and uploads them to PyPI with `uv publish` and Trusted Publishing. See `python/README.md` for supported platforms and required publisher settings. These repository-specific jobs are not part of the reusable Actions.

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

Disable the Release workflow to stop automation. Unmerged Release PR changes do not affect the base branch.

After a partial publication failure, rerun `Release`. The Action uses the saved release commit even after unrelated commits. Do not merge another Release PR until the current batch is fully published: discovery covers only the latest batch. History rewrites or manual batch edits require manual inspection. Never move tags automatically.

If PR creation fails after pushing a new branch, the Action checks for a successful remote PR creation before it removes that branch with a lease. If the API or cleanup also fails, inspect the marked PR and branch manually before retrying; an unowned branch is deliberately rejected. Do not delete a branch without checking its commit and PR history.

Before production use, verify repository permissions, branch rules, and App-token CI behavior in a disposable repository.

## Scope and safety

- Configured version files are the release version source and must contain a stable SemVer version in the field listed above. Only configured files are updated; other manifests, lockfiles, and internal dependencies are **not** updated. Use a package-specific build or publication step if those files require synchronization. The CLI's `--version` reports its Cargo build version, not a configured package version.
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
uv run --no-project python -m unittest discover -s tests/python_release -v
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

The existing [changenote PyPI project](https://pypi.org/project/changenote/) is the publication target. The Release workflow builds platform wheels with maturin Action and publishes with `uv publish`. Configure its PyPI trusted publisher before the first upload. After a successful publication, users on supported platforms can run `uvx changenote status`. See `python/README.md`.

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from root `Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. The PyPI workflow sets the Cargo package and its Cargo.lock entry to the release's `VERSION` in a disposable build checkout. It does not change the committed Cargo files. For manual publication, align the Rust crate version with `VERSION` and update lockfiles first.

CI currently runs on Ubuntu only. It runs Rust format checks, lint checks, and tests, plus Python wrapper tests, binary wheel builds, and uvx checks with Python 3.14. The container image uses the Rust executable; mount a repository at `/repo` to use it.
