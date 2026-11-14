# changenote

A CLI that turns Markdown change notes into SemVer versions, changelogs, and release tags.
Written in Rust. Inspired by [Changesets](https://github.com/changesets/changesets).

- Use one package or a monorepo, in any language.
- Store versions in plain-text files, package manifests, or Xcode configuration files.
- Review a release plan before applying it.
- Release locally or use GitHub Actions for version PRs and publication.

[Install](#install) · [Quick start](#quick-start) · [Commands](#commands) · [GitHub Actions](#github-actions) · [Documentation](#documentation)

## Install

From a local checkout, with stable Rust and edition 2024 support:

```sh
cargo install --path . --locked
```

Git is required for `check` and `tag`.
Plain-text and Xcode configuration version files need no language runtime or package manager.
Manifest releases with a supported existing lockfile need the corresponding [package manager and offline cache](docs/configuration.md#automatic-lockfile-synchronization).

For Python wheels and `uvx` usage, see the [Python distribution guide](python/README.md).

## Quick start

Run commands from your project's repository root, or use `changenote --root PATH <command>`.

### 1. Initialize a package

```sh
changenote init --name pkg-a --version 0.0.0
```

This creates `.changenote/config.json` and a plain-text `VERSION` file.

To use an existing version source instead, run:

```sh
changenote init --name pkg-a --version-file Cargo.toml
```

Use only one initialization command.
See [configuration](docs/configuration.md) for other formats and monorepos.

### 2. Add a change note

```sh
changenote add hello-world --package pkg-a --bump minor --message "Initial release"
```

This creates `.changenote/hello-world.md`:

```markdown
---
pkg-a: minor
---

Initial release
```

Commit the configuration, version file, and note with your change.
Add notes for changes that need a release, according to the repository's policy.
Documentation-only changes, test-only changes, and internal refactors without release impact normally need no note.
Do not create empty notes as no-release markers.
See [when a note is needed](docs/notes.md#decide-whether-a-note-is-needed).

### 3. Review the plan

```sh
changenote status
```

`status` prints JSON without changing files.
For each package, the plan applies the highest requested bump once.
Two patch notes produce one patch bump, not two.

### 4. Apply the release

When the release is approved, start from a clean checkout:

```sh
changenote version
git diff
```

`version` updates version files and supported existing lockfiles, adds changelog entries, saves `.changenote/releases.json`, and removes consumed notes.

For the plain-text `VERSION` example, review and commit those changes, then create the local tag:

```sh
git add -- VERSION CHANGELOG.md .changenote/releases.json .changenote/hello-world.md
git commit -m "chore(release): version packages"
changenote tag
```

The tag is `v0.1.0`.
`tag` requires a clean working tree; it does not push tags or publish artifacts.
See [release behavior and recovery](docs/releases.md) before automating releases.

## Commands

| Command | Purpose |
| --- | --- |
| `init` | Create configuration with a new `VERSION` file or an existing version source. |
| `add` | Create a Markdown change note. |
| `status` | Print the pending release plan as JSON without file changes. |
| `check` | Check committed PR changes for required notes. |
| `doctor` | Report local release prerequisites without file changes. |
| `version` | Apply the plan and consume notes. |
| `release-notes` | Print one package's changelog entry for an exact version. |
| `tag` | Create local annotated tags for the committed release batch. |
| `completion` | Print a Bash, Zsh, or Fish completion script. |

Use `changenote <command> --help` for command options.
The CLI's `--version` reports its build version, not a configured package version.

### Shell completion

`changenote completion <bash|zsh|fish>` prints a completion script to stdout.
It needs no project configuration and does not change files.
The script completes command names, options, and fixed argument values.
Fish does not complete positional argument values.
Generate it again after upgrading changenote.

1. Bash: add this line to `~/.bashrc`, then start a new shell:

   ```bash
   source <(changenote completion bash)
   ```

2. Zsh: add these lines to `~/.zshrc`, then start a new shell:

   ```zsh
   autoload -Uz compinit
   compinit
   source <(changenote completion zsh)
   ```

   If `compinit` is already enabled, add only the `source` line after it.

3. Fish: save the script, then start a new shell:

   ```fish
   mkdir -p ~/.config/fish/completions
   changenote completion fish > ~/.config/fish/completions/changenote.fish
   ```

## GitHub Actions

Choose the Action that matches your publication process:

| Goal | Action | Setup |
| --- | --- | --- |
| Manage version PRs and publish tags and GitHub Releases | `changenote/action/release` | Installs the trusted pinned CLI and prepares package-manager caches. |
| Manage version PRs and run optional project publication scripts | `changenote/action` | Requires an installed compatible CLI and caller-owned tools. |

Actions are maintained in [changenote/action](https://github.com/changenote/action).
Pin Actions to reviewed full commit SHAs from that repository, not CLI commit SHAs.
The root, `/release`, `/check`, `/preview`, and `/publish` entry paths remain supported.
With neither `publish-script` nor `release-script`, the root Action manages PRs without publication.
Actions never approve or merge their PRs.

- [GitHub Actions guide](https://github.com/changenote/action/blob/main/docs/github-actions.md): setup, permissions, inputs, and external CI.
- [Minimal release workflow](https://github.com/changenote/action/blob/main/examples/github-actions/release.yml): built-in tag and GitHub Release publication.
- [Project-script workflow](https://github.com/changenote/action/blob/main/examples/github-actions/command-hooks.yml): caller-owned builds and publication.
- [PR note checks](https://github.com/changenote/action/blob/main/docs/checks.md#github-action-report): read-only checks for contributor PRs.
- [Lower-level Actions](https://github.com/changenote/action/blob/main/docs/github-actions-reference.md): separate PR, preview, and publication operations.
- [Repository migration](https://github.com/changenote/action/blob/main/docs/migration.md): new repository publication and production pin requirements.

Built-in publication creates Git tags and GitHub Releases, not registry uploads.
Keep changelog output enabled for built-in publication.
Use project scripts or separate jobs for registries and other distribution systems.

## Documentation

| Guide | Contents |
| --- | --- |
| [Configuration](docs/configuration.md) | Monorepos, version formats, additional outputs, tags, lockfiles, and version groups. |
| [Change notes and changelogs](docs/notes.md) | Release policy, multiple packages, automatic names, note types, sections, and templates. |
| [Release behavior](docs/releases.md) | Version calculation, saved batches, changelog entries, diagnostics, safety, and recovery. |
| [PR note checks](docs/checks.md) | CLI coverage rules, exemptions, file patterns, and a link to the check Action. |
| [GitHub Actions](https://github.com/changenote/action/blob/main/docs/github-actions.md) | Release workflows, project scripts, tool setup, and external CI. |
| [Go modules](docs/go.md) | Root and nested modules, v2+ paths, and tag prefixes. |
| [Swift packages and Apple apps](docs/swift.md) | Xcode version files, build numbers, and Swift Package Manager tags. |
| [Python distribution](python/README.md) | Wheels, `uvx`, wrapper development, and PyPI publication. |
| [Development](docs/development.md) | Repository layout, local checks, and this repository's release workflow. |

changenote does not update internal dependency requirements, propagate dependency changes, or support prereleases.
Primary and additional version files must contain explicit stable SemVer versions.

## Development

This repository contains one Rust Cargo package and a Python distribution of its executable; neither uses a workspace.
See the [development guide](docs/development.md) for the layout, test commands, and maintainer release procedures.

The project uses the [MIT license](LICENSE).
