# changenote

A Rust CLI for language-independent change notes, SemVer versions, changelogs, and release tags. Inspired by [Changesets](https://github.com/changesets/changesets).

Each package has an explicit directory and a version file: a package manifest or a plain-text file. Plain-text version files need no language runtime or package manager. Manifest releases with an existing supported lockfile need the corresponding package manager.

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

The command updates each configured version file, adds entries to `CHANGELOG.md`, saves the latest release batch to `.changenote/releases.json`, and removes consumed notes. Saved batches contain only `package`, `old_version`, `version`, `bump`, and `tag`; summaries remain in the package changelogs, not in saved `notes` fields. Pending `status` and `version` command output still include notes. New entries follow the existing title and introduction, before the first ATX SemVer version heading outside fenced or indented code. A file without a version heading is treated as an introduction. Prior content is not reformatted. By default, notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`, in that order; empty groups are omitted. Multiline Markdown remains inside each note's list item. A run without pending notes does not change files or clear the saved batch.

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

Manifest versions must be explicit strings. Dynamic Python versions, Poetry-only versions, and inherited Cargo workspace versions are not supported. TOML comments and unrelated fields are preserved. JSON updates replace only the top-level version value token. Every other byte is preserved, including indentation, line endings, field order, number spelling, and the presence or absence of a final newline. Duplicate top-level version keys, including escaped spellings of the same key, are rejected. Dependency requirements are not updated. `init` still creates a plain-text `VERSION`; configure existing manifests manually.

### Automatic lockfile synchronization

`version` refreshes an existing lockfile for each changed configured manifest:

| Manifest | Lockfile | Command |
| --- | --- | --- |
| `Cargo.toml` | `Cargo.lock` | `cargo update --workspace --offline` |
| `pyproject.toml` | `uv.lock` | `uv lock --offline` |
| `package.json` | `package-lock.json` | `npm install --package-lock-only --ignore-scripts --offline --no-audit --no-fund` |

The CLI checks the manifest directory, then its ancestors up to the repository root, and uses the nearest matching lockfile. This supports shared workspace lockfiles. The owning manifest must exist beside the lockfile. All release manifests are updated before any command runs; each shared lockfile is refreshed once. Lockfile symlinks and paths outside the repository are rejected. Plain-text version files do not trigger synchronization. No lockfile is created when no matching file exists. Yarn, pnpm, and npm shrinkwrap files are not supported.

Install the required package manager and populate its cache before running `version`. Commands run offline without upgrade flags; npm lifecycle scripts are disabled and dependencies are not installed. Existing dependency resolutions are retained where the package manager permits. `status`, `add`, and a `version` run without pending notes do not run package managers.

If a command is missing or fails, the CLI restores all changed manifest and lockfile bytes. Changelogs, saved release data, and notes stay unchanged. Filesystem failures can still require Git recovery. Review all outputs before committing. Upgrade the CLI and Actions together: Release PRs stage these lockfiles, and publication reconstructs them with the same commands. Replay therefore needs compatible package-manager versions and cached dependencies. Release trees with the metadata-only batch format but without lockfile updates remain supported.

One note can name multiple packages with different bumps:

```markdown
---
"pkg-a": patch
"pkg-b": minor
---

Update the shared API
```

With two or more configured packages, tags use `changenote/<package>@v<version>`. A patch for `pkg-a` at `0.1.0` creates `changenote/pkg-a@v0.1.1`. Packages without notes remain unchanged unless they belong to an active fixed group. Tag format depends on the number of configured packages, not the number changed in a release.

To create a note with the same bump for several packages:

```sh
changenote add shared-api -p pkg-a -p pkg-b --bump minor -m "Update the shared API"
```

To use different bumps in one note:

```sh
changenote add shared-api --minor pkg-a --patch pkg-b -m "Update the shared API"
```

Repeat `--major`, `--minor`, or `--patch` for each package. Do not combine these options with `--package` or `--bump`. Conflicting bumps for one package and unknown packages fail before a file is created. Repeated identical assignments are deduplicated. The original CLI syntax and Rust `add()` API remain supported; Rust callers can use `add_with_bumps()` for mixed bumps.

## Note types and changelog sections

Classify a note independently of its SemVer bump:

```sh
changenote add fix-parser --package pkg-a --bump patch --type fix -m "Fix parsing of quoted package names."
```

`--type` works with named notes, automatic names, and mixed package bumps.
The note stores one type for all its packages:

```markdown
---
pkg-a: patch
$type: fix
---

Fix parsing of quoted package names.
```

`$type` is metadata, not a package name.
A package named `type` remains supported.
Types use ASCII letters, digits, hyphens, and underscores and are case-sensitive.
No types are built in, and types do not select bumps or decide whether a change needs a release note.
Without section configuration, typed and untyped notes retain the existing bump groups.

Add optional `changelog.sections` to `.changenote/config.json` to group notes by type:

```json
{
  "packages": {
    "pkg-a": { "path": ".", "version_file": "VERSION" }
  },
  "changelog": {
    "sections": [
      { "title": "Features", "types": ["feat"] },
      { "title": "Bug Fixes", "types": ["fix"] },
      { "title": "Performance", "types": ["perf"] },
      {
        "title": "Maintenance",
        "types": ["chore", "docs", "style", "refactor", "test"]
      }
    ]
  }
}
```

Sections follow configuration order, and empty sections are omitted.
Notes within a section follow filename order.
Several types can share one section.
Untyped notes appear last under `Other Changes`, so existing pending notes remain usable.
When section configuration is present, unknown note types are rejected by `add`, `status`, `check`, and `version`.
Sections and their type lists must be nonempty.
Titles must be nonempty, trimmed, unique, and contain no control characters; `Other Changes` is reserved.
A title cannot start with a SemVer version because that would be read as a release heading.
Each type can occur only once across all sections.
Breaking changes still need migration instructions and the bump required by the repository's compatibility policy.
Fixed-group members without direct notes retain their existing explanatory message.

Inspect the type for each direct note or the planned sections without changing files:

```sh
changenote status --include-note-types --include-note-sections
```

`note_types` aligns with `notes` and uses `null` for untyped notes.
`note_sections` contains ordered objects with `title` and `notes`; members without direct notes have empty arrays.
These options work independently of note bump and path options.
Default status JSON and saved release batches do not change.
The Rust APIs `add_with_type()` and `add_generated_with_type()` accept an optional type; existing add APIs remain supported.
`status_with_note_sections()` returns the direct note types and planned sections alongside the existing detailed release.
`load_config()` validates section configuration but returns only package settings; do not use its return value to round-trip a config with sections.

Upgrade the CLI and Actions together before using typed notes or section configuration.
Older CLIs reject the new metadata and configuration.
Release PRs use the same planned sections as changelogs; GitHub Releases use the generated changelog entry.
To remove this feature before versioning, remove section configuration and `$type` metadata from pending notes.
Do not edit committed release batches.

## Note sources and automatic names

```sh
changenote status --include-note-paths
changenote status --include-note-paths --include-note-bumps
changenote add --patch pkg-a -m "Fix output"
changenote add -p pkg-a --bump minor -m "Add an API"
```

`--include-note-paths` adds repository-relative `note_paths` with forward slashes, aligned with direct `notes`. Notes with identical summaries keep their separate paths. The option is independent of `--include-note-bumps`; default status JSON and saved batch fields do not change. Notes are ordered by filename, not creation time. These commands require no Git history and status writes no files. Detailed JSON requires UTF-8 note filenames; default status still supports other filenames.

Omit the positional add name to generate `note-` followed by 32 lowercase hexadecimal characters from OS randomness (`getrandom`). Generated IDs are not chronological. Validation happens before allocation. File allocation is atomic and never overwrites a file or symlink; generated-name collisions retry at most eight times. Other allocation errors fail immediately. Explicit-name collisions still fail without retry. A content-write failure can leave a partial new note; inspect `.changenote` before retrying. The CLI prints the actual path. Rust callers can use `add_generated()`; existing named add APIs and signatures remain unchanged.

## Fixed version groups

Add an optional top-level `fixed` field to the config:

```json
{
  "packages": {
    "pkg-a": { "path": "packages/pkg-a", "version_file": "VERSION" },
    "pkg-b": { "path": "packages/pkg-b", "version_file": "package.json" }
  },
  "fixed": [["pkg-a", "pkg-b"]]
}
```

Each group must contain at least two distinct known names. Groups cannot overlap; globs, unknown names, invalid types, and duplicate memberships are rejected. Absent `fixed` and `fixed: []` mean independent versions. The existing Rust `Config` struct remains a package view: `load_config()` validates fixed settings, but serializing its return value does not preserve them. Do not use that view to round-trip a config with fixed settings.

When any member has a pending note, the planner takes the highest current version in the group, applies the highest requested member bump once, and releases every member at that version. Each result keeps its actual `old_version`. For example, members at `1.0.0` and `2.4.9` with a patch note release together at `2.4.10`. The change for a lower-version member can therefore exceed the requested bump. A group without pending notes does not synchronize differing versions.

Direct summaries, their bumps, and their paths stay with the named package. A propagated member has empty `notes` and direct `note_paths`. Detailed status adds `fixed_group` with sorted `members`, `base_version`, and filename-ordered `source_note_paths` for all notes that activate the group. Those sources are not direct package notes. Rust callers can use `status_with_note_paths()` without changing the existing `status()` API or public struct layouts.

All member version files, changelogs, and tags are release outputs. A member without direct notes gets the text “Released with its fixed version group; no direct changes.” in its changelog, Release PR, and GitHub Release. Dependency ranges remain unchanged; supported existing lockfiles are synchronized. Upgrade CLI and Actions together before using fixed config; older CLIs reject it.

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

This repository provides three reusable composite Actions:

- `narumiruna/changenote@<full-commit-sha>`: create or update a Release PR.
- `narumiruna/changenote/publish@<full-commit-sha>`: publish annotated tags and GitHub Releases.
- `narumiruna/changenote/preview@<full-commit-sha>`: inspect publication state without source or remote writes.

The PR and publication Actions require Linux, Bash, Git, `gh`, `jq`, a clean checkout, and an installed `changenote` executable on `PATH`. Ubuntu GitHub-hosted runners include the non-CLI tools. Package version files, lockfiles, changelogs, and `.changenote/releases.json` must not be symlinks. Both writing Actions check the batch path before CLI writes. Run at the repository root; the Actions load their scripts from the Action checkout, not the consumer repository. They do not install the CLI or publish registries. The CLI updates configured version files and supported existing lockfiles; it does not synchronize other manifests. Manifest releases also need the package managers and cached dependencies listed above.

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

Release PR descriptions use a `Releases` heading and `package@version` headings. By default, notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`; empty groups are omitted. Multiline notes remain within their list items. The Action requires a CLI that supports `status --include-note-sections`. This option adds ordered `note_sections` objects with `title` and `notes`; default release-plan output and saved batches are unchanged. Custom section configuration replaces the bump groups for both Release PRs and changelogs.

Existing branches require a marked PR in their history. Lease-based pushes stop if another actor moves the branch. The Action stages only release outputs and consumed notes. Notes and titles are data, not shell commands. PR bodies over 60,000 bytes omit release details.

In repository settings, enable **Actions > General > Allow GitHub Actions to create and approve pull requests**. Branch rules must permit the bot to push the release branch. The Action never approves or merges its PR.

PRs created with the default `GITHUB_TOKEN` do not automatically trigger PR workflows. If CI must run on the Release PR, use a GitHub App token (recommended) or PAT with repository contents and pull requests write permissions. Pass it through `github-token`. Checkout credentials do not select the Action's token.

### Publication preview

The preview Action requires Linux, Bash, Git, `gh`, `jq`, an installed compatible CLI, and standard coreutils. It needs only `contents: read`. It reads the selected remote branch in a temporary repository; the caller's files, index, refs, tags, and Git identity are unchanged. A dirty or stale source checkout does not select the release. Temporary fetches and release-tree reconstruction are removed on success and failure.

```yaml
name: Publication preview
on:
  workflow_dispatch:
permissions:
  contents: read
jobs:
  preview:
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote/preview@<full-action-commit-sha>
        with:
          base-branch: main
```

This is a template, not part of the production release workflow. Use the same reviewed compatible full SHA for CLI and Action. Preview uses the same latest-batch discovery, full-tree verification, and remote-state inspection as publication. It never creates tags, commits, PRs, or Releases and does not inspect registries.

The `publication-plan` output is a compact JSON array of `name`, `version`, `tag`, `action`, and `conflicts`. Actions are `create-tag-and-release`, `create-release`, `none`, or `conflict`. Conflicts include a wrong remote tag identity, a draft Release, or a Release without its remote tag. Every batch entry is inspected before a conflict report is emitted. The `ready` output is true only after complete inspection finds no conflicts. `release-commit` identifies the verified batch commit.

No batch succeeds with `ready=true`, an empty commit, and `[]`. Conflicts return the complete inspected report with `ready=false` and fail the step. Invalid input, tree/batch validation failures, or unavailable remote data fail without a ready plan; use the error diagnostic, not an empty plan, as the result. A readable summary goes to the step log. API failures are never treated as missing releases.

The preview is advisory, not a reservation. Remote state can change afterward; real publication repeats preflight and race checks. Preview does not grant permission to publish.

### Publication workflow

The generic consumer workflow above runs publication after PR management on every base-branch push. Publication can recover an earlier batch even if PR management fails. Its job needs only `contents: write`. The repository's own workflow adds the automatic publication gate described below.

In the generic consumer workflow, manual dispatch runs both PR management and publication again. In this repository, run **Release** manually on `main` with `operation=retry-publication` to retry a partial failure. The default `operation=manage-pr` only manages the Release PR. All operations use one workflow concurrency group. Keep required publication steps in this workflow: tags pushed with `GITHUB_TOKEN` do not trigger other push workflows.

Publication selects the latest first-parent commit that changes `.changenote/releases.json`. It reconstructs that commit from the parent's notes and compares the complete generated tree before publication. The publication Action checks current output, then output without lockfile synchronization. Both profiles require metadata-only saved batches. Mixed output profiles and arbitrary edits are rejected. The hidden `version --no-lockfiles` option is reserved for reconstruction in a temporary Action worktree; do not use it for new releases. Standard merge, squash, and rebase merges are supported. A later unrelated commit or new pending notes do not change the release target.

Each GitHub Release body comes from the corresponding package's `CHANGELOG.md` entry in the validated release commit. The package's configured directory selects the file, and the saved version selects the entry. Publication removes the version heading but preserves the Markdown body, including bump groups, nested lists, links, and code blocks. Empty groups are absent because changelog generation omits them. All package bodies are checked before any tag or Release write. Missing files, missing or duplicate version entries, and empty bodies stop publication without a saved-notes fallback. Later changelog edits do not change the selected body. Fixed-group members without direct notes keep their changelog's explanatory message.

To inspect an entry without changing files, run `changenote release-notes --package pkg-a --version 1.2.0`. This command reads the local tree; the publication and preview Actions run it only in the verified release worktree. Extraction and changelog insertion accept ATX SemVer headings with up to three leading spaces outside code fences and generated note list items. Generated note continuation lines, including blank lines, have two leading spaces. Unindented separators end those items. Thus nested headings remain in the body, while indented release headings start separate entries. Leading and trailing blank lines are removed, and the output ends with a newline.

**Breaking saved-batch change:** upgrade the CLI and all Action pins together. Old saved batches containing `notes` are unsupported for tag creation, preview, and publication. Complete old unpublished batches with their matching old tooling before upgrade, or handle them manually after inspecting release state. Do not edit committed batch data to convert it: full-tree verification rejects manual changes. The next authorized `version` operation writes the new format. Existing GitHub Release text is never rewritten.

Existing remote tags must point to the selected commit. Missing tags and GitHub Releases are created; existing release text is not replaced. Draft releases, missing tags for existing releases, malformed batches, batch deletion, manually edited release trees, and tag conflicts stop publication. A batch in a root commit cannot be verified. No batch is a no-op.

The repository's `.github/workflows/release.yml` only manages the Release PR on ordinary pushes to `main`, even when an older release batch exists. Automatic publication requires a new batch-changing first-parent commit in the push range and exactly one associated merged managed PR. The PR must use the reserved head branch, the repository's `main` base, and the managed body marker. Its merge commit SHA must match the batch commit. PR titles and commit messages do not authorize publication. Force pushes, missing history, invalid batches, ambiguous associations, and API errors stop routing without authorizing publication.

Before publication, the workflow previews and validates the latest remote batch. Automatic runs require that batch to match the routed commit; publication checks the selection again after its own fetch and before any remote writes. If a newer release has replaced it, the run fails and requires an explicit retry. Multiple releases in one push still select only the latest batch. An unrelated later commit does not change the selected batch.

Manual `operation=retry-publication` skips PR management and retries the latest validated batch. Wheels run after successful publication with a non-empty validated commit, even if `published=false` because the GitHub tag and Release already exist. A missing batch is a no-op. Cancellation or failed publication prevents wheel builds and PyPI uploads. Retry builds can differ from files already on PyPI: `uv publish --check-url` skips matching files but rejects different contents for the same filename. Inspect registry state and resolve conflicts separately; never replace published files automatically.

The workflow builds the CLI instead of installing it and does not push version commits directly to `main`. It builds Python distributions from the validated release commit and uploads them to PyPI with `uv publish` and the `PYPI_TOKEN` GitHub Actions secret. See `python/README.md` for supported platforms and required publisher settings. This routing policy is repository-specific; the reusable Actions keep their historical-batch recovery behavior.

### Inputs and outputs

| Action | Input | Default |
| --- | --- | --- |
| All | `github-token` | Workflow token |
| All | `base-branch` | Repository default branch |
| Release PR | `commit-message` | `chore(release): version packages` |
| Release PR | `pr-title` | `chore(release): version packages` |

The PR Action outputs `has-notes` (`true` or `false`), `pr-number` (number or empty string), and `pr-operation` (`created`, `updated`, `closed`, or `none`). `updated` includes a no-op version rerun that refreshes the PR metadata.

The publication Action outputs `release-commit` (commit SHA or empty string), `published` (`true` if this run created any remote tag or GitHub Release), and `published-packages` (JSON, for example `[{"name":"pkg-a","version":"1.2.0"}]`). This is not registry publication. Read outputs only after successful steps.

### Recovery and limits

Disable the Release workflow to stop automation. Unmerged Release PR changes do not affect the base branch.

After a partial publication failure in this repository, run `Release` on `main` with `operation=retry-publication`. The Action uses the saved release commit even after unrelated commits. Do not merge another Release PR until the current batch is fully published: discovery covers only the latest batch. History rewrites or manual batch edits require manual inspection. Never move tags automatically.

If PR creation fails after pushing a new branch, the Action checks for a successful remote PR creation before it removes that branch with a lease. If the API or cleanup also fails, inspect the marked PR and branch manually before retrying; an unowned branch is deliberately rejected. Do not delete a branch without checking its commit and PR history.

Before production use, verify repository permissions, branch rules, and App-token CI behavior in a disposable repository.

## Scope and safety

- Configured version files are the release version source and must contain a stable SemVer version in the field listed above. Configured version files and supported existing lockfiles are updated; other manifests and internal dependency requirements are **not** updated. Use a package-specific build or publication step if those files require synchronization. The CLI's `--version` reports its Cargo build version, not a configured package version.
- Names use ASCII letters, digits, hyphens, and underscores. Paths are relative to the repository root and cannot contain parent components. Package output files cannot overlap or resolve outside the repository.
- Unknown packages, invalid bumps, duplicate YAML keys, empty summaries, and prerelease versions are rejected. `add` never overwrites a note.
- Only top-level `.changenote/*.md` files are read. Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
- `tag` requires a clean Git working tree and committed release data. It creates local tags only; it does not push, publish artifacts, or run arbitrary commands.
- Input validation and output preparation happen before version writes. Filesystem failures during writes can leave partial changes. Run in a clean checkout, inspect changes, and use Git to restore files before retrying. Do not run concurrent local version commands.
- Dependency propagation, registry publication, prereleases, and linked version groups are not implemented.

## Development

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
- `action.yml`, `publish/action.yml`, `preview/action.yml`: reusable Linux GitHub Actions.
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

The existing [changenote PyPI project](https://pypi.org/project/changenote/) is the publication target. The Release workflow builds platform wheels with maturin Action and publishes with `uv publish`. Configure its `PYPI_TOKEN` GitHub Actions secret before the first upload. After a successful publication, users on supported platforms can run `uvx changenote status`. See `python/README.md`.

The Rust CLI and Python distribution share the `changenote` release plan and root `CHANGELOG.md`; the current single release package uses `v<version>` tags. Python has no independent version file or bump. Its manifest uses a dynamic version from root `Cargo.toml`, so Python wheel metadata and the included Rust CLI version match. The PyPI workflow validates `Cargo.toml` against the saved release batch and checks the local package version in `Cargo.lock`. Older releases configured with `VERSION` receive build-only Cargo version synchronization in a disposable checkout. The wheel check uses the validated version, not a separate version file. For manual publication, update the Cargo version and lockfile first.

CI currently runs on Ubuntu only. It runs Rust format checks, lint checks, and tests, plus Python wrapper tests, binary wheel builds, and uvx checks with Python 3.14. The container image uses the Rust executable; mount a repository at `/repo` to use it.
