# Change notes and changelogs

Use notes to record release changes with the implementation.
A note selects affected packages and bumps; optional types control changelog sections, not versions.

- [Decide whether a note is needed](#decide-whether-a-note-is-needed)
- [Note format](#note-format)
  - [Write for the generated changelog](#write-for-the-generated-changelog)
- [Notes for multiple packages](#notes-for-multiple-packages)
- [Note sources and automatic names](#note-sources-and-automatic-names)
- [Automatic commit source links](#automatic-commit-source-links)
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
Keep verified implementation PR links in the body; do not guess unknown PR numbers.

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

### Write for the generated changelog

Write each note as an entry in the combined `CHANGELOG.md`.
The renderer adds version headings, section headings, and one list item for each note.
It indents continuation lines inside that item.
Consecutive prose lines form one paragraph, not separate changelog entries.
A list inside a note becomes a nested list, not separate entries.

- Start with a short plain-text summary of the change's effect on users.
  - Use backticks for commands, paths, option names, and API names.
- Use a separate note for each independent change, even within one PR or with the same package and bump.
  - Update an existing note only for the same change.
  - Keep tightly related details together; do not split notes by sentence count or paragraph length.
- Keep required user actions, migration steps, and known limits with the change they explain.
  - Include only instructions and limits supported by the change evidence.
- Do not add release headings or an outer list marker just to format the generated entry.
  - Use nested lists or numbered steps when needed for related details or migration instructions.

These are writing guidelines, not additional validation rules.
Markdown headings, lists, and multiple paragraphs remain supported.

Select the appropriate bump for each independent change.
With default grouping, each note appears under its own bump, even when the package receives a higher bump.
With configured sections, the note's type selects its section independently of its bump.
A fix inside a feature note does not become a separate patch entry.

#### Combined changelog example

This example starts with `pkg-a` at `1.0.0` and uses default bump grouping.
It has no source links or custom note template.

Example `.changenote/message-file.md`:

```markdown
---
pkg-a: minor
---

Read change note messages from a file.
Use `--message-file <path>` to supply a Markdown file.
```

Example `.changenote/quoted-package-names.md`:

```markdown
---
pkg-a: patch
---

Fix parsing of quoted package names in change notes.
```

The combined `CHANGELOG.md` entry is:

```markdown
## 1.1.0

### Minor Changes

- Read change note messages from a file.
  Use `--message-file <path>` to supply a Markdown file.

### Patch Changes

- Fix parsing of quoted package names in change notes.
```

The feature's required user instruction remains inside its item.
The independent fix receives its own item under `Patch Changes`.
Optional `Sources:` paragraphs and template metadata also remain inside each note's generated item.
Do not add them manually; see [source links](#automatic-commit-source-links) and [templates](#changelog-output-controls-and-native-templates).

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

## Automatic commit source links

Set `changelog.source_repository` to enable links from each direct note to its local Git history:

```json
{
  "changelog": {
    "source_repository": "https://github.com/owner/repo"
  }
}
```

Use a canonical HTTPS GitHub owner/repository URL, without a trailing slash or `.git` suffix.
Credentials, query strings, fragments, other hosts, and invalid paths are rejected.
Sections remain optional; source links also work with default bump groups.
Without this field, output stays unchanged and no source-history lookup runs.

The CLI checks that the current note content matches a regular file in `HEAD`, using the parser's CRLF-to-LF normalization.
Clean CRLF checkouts retain their sources; other content differences remain unlinked.
The check does not execute Git clean filters or textconv commands, and user `log.follow` settings do not change attribution.
It selects note changes on first-parent history, including creation and later edits.
A merge that changes the note is a source on that branch; internal side-branch commits are not separate sources.
Unrelated later commits do not change the links.
The CLI follows actual renames at a fixed 50% similarity threshold, but not copies of identical files.
A deletion followed by recreation, or a file-type change, starts a new note history.
A move rewritten beyond that similarity threshold is treated as a new file.
Sources are deduplicated and sorted by full commit object ID, not time.
The link label uses the first 12 characters; its target uses the full object ID.

Each rendered note starts with a `Sources:` paragraph, followed by the original summary.
This keeps links outside fenced or indented code, including an unclosed fence.
Source attribution runs before `note_template`; native section and list rendering run after it.
Raw `notes`, note paths, bumps, public Rust layouts, and saved release batches do not change.
Group-only releases have no fabricated direct sources.
With `enabled: false`, planned sections still show sources, but changelogs remain unchanged and built-in publication remains unsupported.

These links identify commits that changed the note, not necessarily the complete implementation.
Keep verified PR links in the summary when useful; several PR links can explain one change.
No GitHub API lookup, remote inference, or network request is used to find sources.
The configured repository URL must identify the repository that contains this history.

`add` and `status` remain usable before a note is committed.
Untracked or changed notes, missing Git, source archives, and shallow or incomplete history have no automatic link in status.
When enabled, `version` rejects any consumed note without provable sources before writing files or deleting notes.
Unrelated paused notes do not block active releases.
Commit the notes and provide complete local history before versioning; the CLI does not fetch it.

Upgrade the CLI used for generation, preview, and publication before enabling this field.
Older CLIs reject it.
Use the same exact compatible CLI version in each stage.
To disable source links before versioning, remove the field.
For an existing release batch, keep compatible tooling and its historical configuration for replay; do not edit the generated tree.

### Separate change examples

Use separate notes for repository migration and CLI installation changes:

```markdown
---
pkg-a: minor
---

Move the Actions to the dedicated Action repository.
Update workflow references to the new repository and use a reviewed Action commit SHA.
```

```markdown
---
pkg-a: minor
---

Install the CLI from a prebuilt PyPI wheel with uv.
Remove separate CLI installation steps.
Set the same exact compatible CLI version in each release job.
```

Replace package names and instructions with the actual change evidence.
Add only verified source links and current compatibility limits.
These examples do not change published release text or establish that a particular CLI version is available.

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
