# Release behavior

Review the release plan before changing files.
Use a clean checkout and run only one local version command at a time.

- [Version calculation](#version-calculation)
- [Release outputs](#release-outputs)
- [Changelog entries](#changelog-entries)
- [Read a release entry](#read-a-release-entry)
- [Local tags](#local-tags)
- [Local diagnostics](#local-diagnostics)
- [Release-write recovery](#release-write-recovery)
- [Scope and safety](#scope-and-safety)

## Version calculation

`status` prints the pending plan as JSON without changing files.
`version` applies the plan.

For each package, `version` takes the highest requested bump and applies it once.
Multiple patch notes cause one patch bump, not multiple bumps.
Major, minor, and patch follow stable SemVer rules, including versions below 1.0.0.

Packages without notes remain unchanged unless they belong to an active fixed group.
See [fixed and linked version groups](configuration.md#fixed-version-groups) for group rules.

## Release outputs

A `version` run with pending releases:

1. Updates configured primary and additional version files.
2. Refreshes supported existing lockfiles.
3. Adds entries to each package's `CHANGELOG.md`, unless output is disabled.
4. Saves the latest release batch to `.changenote/releases.json`.
5. Removes consumed notes.

Saved batches contain only `package`, `old_version`, `version`, `bump`, and `tag`.
With default output, summaries remain in package changelogs, not saved `notes` fields.
Pending `status` and `version` command output still include notes.

A run without pending releases does not change files or clear the saved batch.
Paused notes remain pending.

For lockfile tools, offline cache requirements, and supported formats, see [automatic lockfile synchronization](configuration.md#automatic-lockfile-synchronization).

## Changelog entries

New entries follow the existing title and introduction, before the first ATX SemVer version heading outside fenced or indented code.
A file without a version heading is treated as an introduction.
Prior content is not reformatted.

By default, notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`, in that order.
Empty groups are omitted.
Multiline Markdown remains inside each note's list item.

You can configure [note types and changelog sections](notes.md#note-types-and-changelog-sections) or [output controls and templates](notes.md#changelog-output-controls-and-native-templates).
Keep changelog output enabled for built-in GitHub publication.
Disabling output does not preserve consumed summaries elsewhere.

Optional [commit source links](notes.md#automatic-commit-source-links) use the checked-out note history and `changelog.source_repository`.
Source checks for all consumed notes finish before version files, lockfiles, changelogs, or saved batches change.
Missing source evidence is an error for versioning, not permission to use the current branch tip.
Publication reconstructs links from the selected historical parent and compares the complete release tree.
A later configuration or remote change does not replace the historical repository URL.
If the linked tree cannot be reproduced, verification fails before remote writes.
GitHub Release text is extracted from the verified committed changelog, not regenerated with new source lookups.

## Read a release entry

```sh
changenote release-notes --package pkg-a --version 1.2.0
```

This command reads the local tree.
The publication and preview Actions run it only in the verified release worktree.

Extraction and changelog insertion use these rules:

- ATX SemVer headings can have up to three leading spaces outside code fences and generated note list items.
- Generated note continuation lines, including blank lines, have two leading spaces.
  - Unindented separators end those items.
  - Nested headings remain in the body, while indented release headings start separate entries.
- Leading and trailing blank lines are removed.
  - The output ends with a newline.

## Local tags

Commit and review the release outputs before tagging.
`tag` requires a clean Git working tree and committed release data.
It creates local annotated tags for the saved batch; it does not push them or publish artifacts.

```sh
changenote tag
```

A single-package release from `0.0.0` with a minor bump creates `v0.1.0`.
After publication approval, push the commit and tag:

```sh
git push origin HEAD
git push origin refs/tags/v0.1.0
```

See [default tag names](configuration.md#default-tag-names) and [custom tag prefixes](configuration.md#additional-version-files-and-tag-prefixes).
Do not edit committed release batches or move published tags.

## Local diagnostics

```sh
changenote doctor
```

`doctor` prints JSON with `checks` and `passed`. Each check has `name`, `status`, and `message`.
Statuses are `pass`, `fail`, or `unknown`; any failed check produces exit code 1.
It checks configuration, notes, the pending plan, output path types, existing supported lockfiles, and required package-manager executables on `PATH`.
Tool checks apply only to pending synchronization work, including additional version outputs.
It does not execute package managers, install tools, test write access, or contact remote services.

Local Git checks do not refresh the index or run fsmonitor hooks.
A dirty tree or a nested Git root fails the Git check; unavailable Git or a non-Git directory is reported as unknown.
Cache and remote credential readiness remain unknown.
Thus `passed: true` means no local check failed, not that publication is ready or authorized.
The current manifest-to-lockfile rules still apply; automatic package-manager selection is separate work.
Rust callers can use `doctor()` and its structured report types.

## Release-write recovery

Before writes, `version` saves the original bytes of all known release outputs and consumed notes in memory.
Absent changelogs or batch files are also recorded.
If synchronization, a later write, or note deletion fails, the CLI attempts every restoration.
It recreates removed notes and removes newly created output files. It does not remove unrelated files or directories.
Symlink files and parent directories are rejected. Unsafe replacement paths are not followed during restoration.

A recovery failure reports the affected paths and the original error. Inspect Git state before retrying.
This is best-effort recovery for ordinary errors, not a multi-file atomic operation.
Process termination, power loss, storage failure, concurrent changes, and unlisted files changed by external tools are outside the guarantee.
Use a clean checkout, review the result, and do not run concurrent local version commands.

## Scope and safety

- Primary version files are the release version source.
  - Use a stable SemVer version in the [supported version field](configuration.md#package-configuration).
  - The CLI's `--version` reports its Cargo build version, not a configured package version.
- Releases update configured primary and additional version files and supported existing lockfiles.
  - Unlisted manifests and internal dependency requirements are **not** updated.
  - Use a package-specific build or publication step if those files require synchronization.
- Names use ASCII letters, digits, hyphens, and underscores.
- Paths are relative to the repository root and cannot contain parent components.
  - Package output files cannot overlap or resolve outside the repository.
- Unknown packages, invalid bumps, duplicate YAML keys, empty summaries, and prerelease versions are rejected.
  - `add` never overwrites a note.
- Only top-level `.changenote/*.md` files are read.
  - Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
- `tag` requires a clean Git working tree and committed release data.
  - It creates local tags only; it does not push, publish artifacts, or run arbitrary commands.
- Input validation and output preparation happen before version writes.
  - Ordinary failures trigger best-effort recovery of known release files.
  - Failed recovery or process termination can leave partial changes.
  - Run in a clean checkout, inspect changes, and use Git to restore files before retrying.
  - Do not run concurrent local version commands.
- The CLI and built-in Action publication paths do not implement dependency propagation, registry clients, or prereleases.
  - Optional root Action commands can invoke caller-owned registry tools.
