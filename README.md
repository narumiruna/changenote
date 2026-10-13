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

This repository includes `.github/workflows/release.yml`. Run **Release** from the Actions page after notes are merged into `main`.

The workflow:

1. Tests and builds the Rust CLI.
2. Applies pending notes and commits the version files, changelogs, and release batch to `main`.
3. Creates and pushes one annotated tag for each changed package.
4. Creates a GitHub Release for each tag, with the note summaries as release notes.

It requires `contents: write` and a branch rule that permits the workflow to push the release commit. It uses `GITHUB_TOKEN`; no registry token is required. Tags pushed with that token do not trigger other push workflows. Put required publication steps in the release workflow instead of relying on a tag-push event.

To use the workflow in a repository that does not contain the Rust CLI source, replace its test/build step with:

```yaml
- uses: dtolnay/rust-toolchain@stable
- name: Install changenote
  run: cargo install --git https://github.com/narumiruna/changenote --rev <full-commit-sha> --locked
```

Replace `./target/release/changenote` with `changenote`, configure packages, and change `main` if necessary. Pin a reviewed commit; the crate is not yet published to crates.io.

The saved batch supports retries after the version commit. Existing tags are accepted only when they point to the current commit. Existing GitHub Releases are not replaced. Retry before unrelated commits reach `main`; otherwise inspect the release commit and finish publication from that commit. Tags are never moved automatically.

## Scope and safety

- Version files contain only a stable SemVer string. They are the release version source; `Cargo.toml`, `package.json`, `pyproject.toml`, lockfiles, and internal dependencies are **not** updated. Use a package-specific build or publication step if those files require synchronization. The CLI's `--version` reports its Cargo build version, not a configured package version.
- Names use ASCII letters, digits, hyphens, and underscores. Paths are relative to the repository root and cannot contain parent components. Package output files cannot overlap or resolve outside the repository.
- Unknown packages, invalid bumps, duplicate YAML keys, empty summaries, and prerelease versions are rejected. `add` never overwrites a note.
- Only top-level `.changenote/*.md` files are read. Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
- `tag` requires a clean Git working tree and committed release data. It creates local tags only; it does not push, publish artifacts, or run arbitrary commands.
- Input validation and output preparation happen before version writes. Filesystem failures during writes can leave partial changes. Run in a clean checkout, inspect changes, and use Git to restore files before retrying. Do not run concurrent local version commands.
- Dependency propagation, registry publication, prereleases, and fixed or linked version groups are not implemented.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked --release
```

CI runs format checks, lint checks, and tests on Linux, macOS, and Windows. The container image uses the Rust executable; mount a repository at `/repo` to use it.
