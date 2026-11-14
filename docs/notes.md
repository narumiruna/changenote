# Change notes and changelogs

Use notes to record release changes with the implementation.
A note selects affected packages and bumps; optional types control changelog sections, not versions.

- [Decide whether a note is needed](#decide-whether-a-note-is-needed)
- [Note format](#note-format)
- [Notes for multiple packages](#notes-for-multiple-packages)
- [Note sources and automatic names](#note-sources-and-automatic-names)
- [Note types and changelog sections](#note-types-and-changelog-sections)
- [Changelog output controls and native templates](#changelog-output-controls-and-native-templates)

## Decide whether a note is needed

Follow the repository's release policy and check the change's effect on users and release artifacts, not only its file type.

- If a release is needed, add or update a note with a configured package, a valid bump, and a nonempty summary of the effect on users.
- If no release is needed, create no note and state the reason in the completion report or PR description.
  - Documentation-only changes, test-only changes, and internal refactors without release impact normally need no note.

Do not use empty notes as no-release markers or invent a package bump or summary.
Empty files, whitespace-only files, delimiter-only notes, and notes without package assignments or a nonempty summary are invalid.
For changes that need a check exemption, see [Check PR notes](checks.md).
An exemption removes the missing-note requirement, not validation of pending notes.

## Note format

A note is a Markdown file in the top level of `.changenote/`.
The front matter assigns a SemVer bump to each affected package.
The body describes the change's effect on users.

```markdown
---
pkg-a: minor
---

Add an API for shared configuration.
```

Quoted keys, such as `"pkg-a": minor`, are also accepted.
You can write notes directly without the CLI.
Only top-level `.changenote/*.md` files are read.
Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.

## Notes for multiple packages

One note can name multiple packages with different bumps:

```markdown
---
"pkg-a": patch
"pkg-b": minor
---

Update the shared API
```

To create a note with the same bump for several packages:

```sh
changenote add shared-api -p pkg-a -p pkg-b --bump minor -m "Update the shared API"
```

To use different bumps in one note:

```sh
changenote add shared-api --minor pkg-a --patch pkg-b -m "Update the shared API"
```

Repeat `--major`, `--minor`, or `--patch` for each package. Do not combine these options with `--package` or `--bump`. Conflicting bumps for one package and unknown packages fail before a file is created. Repeated identical assignments are deduplicated. The original CLI syntax and Rust `add()` API remain supported; Rust callers can use `add_with_bumps()` for mixed bumps.

## Note sources and automatic names

```sh
changenote status --include-note-paths
changenote status --include-note-paths --include-note-bumps
changenote add --patch pkg-a -m "Fix output"
changenote add -p pkg-a --bump minor -m "Add an API"
```

`--include-note-paths` adds repository-relative `note_paths` with forward slashes, aligned with direct `notes`. Notes with identical summaries keep their separate paths. The option is independent of `--include-note-bumps`; default status JSON and saved batch fields do not change. Notes are ordered by filename, not creation time. These commands require no Git history and status writes no files. Detailed JSON requires UTF-8 note filenames; default status still supports other filenames.

Omit the positional add name to generate `note-` followed by 32 lowercase hexadecimal characters from OS randomness (`getrandom`). Generated IDs are not chronological. Validation happens before allocation. File allocation is atomic and never overwrites a file or symlink; generated-name collisions retry at most eight times. Other allocation errors fail immediately. Explicit-name collisions still fail without retry. A content-write failure can leave a partial new note; inspect `.changenote` before retrying. The CLI prints the actual path. Rust callers can use `add_generated()`; existing named add APIs and signatures remain unchanged.

### Read a note message from a file or stdin

```sh
changenote add --minor pkg-a --message-file migration.md
printf '%s\n' 'Fix output formatting.' | changenote add --patch pkg-a --message-file -
```

`--message-file` accepts UTF-8 Markdown and is mutually exclusive with `--message`.
A file path is relative to the current directory, not `--root`; `-` reads stdin.
Named notes, automatic names, mixed bumps, and `--type` remain available.
Input read errors, invalid UTF-8, and empty messages fail before a note is created.
Multiline Markdown uses the same whitespace rules as inline messages.
No editor or shell command is run to interpret the message.

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

## Changelog output controls and native templates

```json
{
  "changelog": {
    "enabled": true,
    "note_template": "{package}@{version} [{bump}]\n\n{summary}"
  }
}
```

Both fields are optional. `enabled` defaults to `true`, and `note_template` defaults to `{summary}`.
Existing sections-only configuration remains supported; `sections` is optional when only output controls are used.
An empty changelog object retains default bump grouping. If sections are supplied, they must still be nonempty and valid.

A template is either exactly `{summary}`, or one metadata line followed by a blank line and `{summary}`.
The metadata line must be nonempty, trimmed, contain no control characters, and not start a backtick or tilde code fence.
Metadata supports `{package}`, `{version}` (the target version), and `{bump}` (that note's own bump).
Unknown or unbalanced placeholders, literal braces in metadata, and null values are rejected before writes.
The complete summary is inserted once as literal Markdown; placeholders or shell syntax inside it are not evaluated.
Version headings, section headings, and list indentation stay under the native renderer's control.
Rendered `note_sections` and Release PR details use the template. Raw `notes`, bumps, and paths do not change.
Members without direct notes retain their native explanatory message. No JS module, shell hook, formatter, or network lookup is run.

With `"enabled": false`, version files and saved batch metadata still update, and consumed notes are removed, but changelogs are neither created nor changed.
An existing changelog is not a release-body fallback. Local tagging remains supported.
The built-in publication paths in `/preview`, `/publish`, and `/release` do not support batches generated with changelog output disabled; they fail before remote tag or Release writes.
This decision uses the selected historical batch's configuration, not later live settings.
Keep changelogs enabled for built-in GitHub publication.
The root Action's custom `publish-script` and `release-script` do not require changelog output.
Disabling output does not preserve consumed summaries elsewhere.
Upgrade CLI and all Action pins together before using these controls.

For version calculation, saved batches, and changelog insertion, see [Release behavior](releases.md).
