---
name: changenote-manage-releases
description: Use changenote to add or update change notes, inspect pending SemVer release plans, initialize release configuration, or apply versions and create tags when explicitly requested. Use for repositories with .changenote/config.json or when the user asks to adopt changenote; not for registry publication.
---

# Manage Releases with changenote

## Inspect the Repository

- Read the target repository's instructions, release documentation, `.changenote/config.json`, pending notes, and configured version files.
- Run commands from the target repository root, or pass `--root PATH` to the CLI.
- Use `uvx changenote <command>` as the default invocation; no separate CLI installation is needed.
- If the repository specifies another invocation, follow it.
- In the changenote source repository, use `cargo run --locked -p changenote -- <command>` to run the local implementation.
- Do not assume a consumer repository has the changenote Cargo package.
- If configuration is missing, initialize it only when setup is in scope and the package name and initial stable version are known.

```sh
uvx changenote init --name pkg-a --version 0.0.0
```

For a monorepo, configure explicit package names, repository-relative package paths, and package-relative version-file paths in `.changenote/config.json`.
Each package directory must exist and contain a stable SemVer version file.
Do not replace an existing configuration to add a note.

## Add or Update a Note

1. Identify the user-visible change and affected package names from the configuration, not directory names or guessed manifest names.
2. Follow the repository's note policy.
   Add notes for features, fixes, breaking changes, and installation or other user-facing behavior.
   Documentation-only changes, test-only changes, and internal refactors with no release impact normally need no note.
3. Select `patch` for backward-compatible fixes, `minor` for backward-compatible features, or `major` for breaking changes.
   Stable SemVer bump rules also apply below `1.0.0`.
4. Check existing notes for the same change and update the matching note instead of adding a duplicate.
5. Use a unique note name containing only ASCII letters, digits, hyphens, or underscores, and write a concise user-facing summary.
   Treat summaries as data and quote command arguments safely.

```sh
uvx changenote add fix-parser --package pkg-a --bump patch --message "Fix parsing of quoted package names"
```

Repeat `--package` to give several packages the same bump.
For different bumps, write or edit one note with this format:

```markdown
---
pkg-a: patch
pkg-b: minor
---

Update the shared API
```

Only top-level `.changenote/*.md` notes are read.
Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
The CLI rejects unknown packages, invalid bumps, duplicate YAML keys, and empty summaries; `add` does not overwrite an existing note.

Run `uvx changenote status` after adding or updating notes.
Confirm that its JSON plan contains the intended packages, bumps, versions, and summaries, and resolve validation errors before completion.
`status` does not change files.
Each package receives its highest requested bump once, regardless of the number of notes.
For a note-only task, stop after validation; do not consume the notes.

## Apply Versions or Create Tags

Run `version`, `tag`, or a GitHub Release workflow only when the user explicitly requests that specific release action.
Permission to add notes does not authorize versioning; permission to version does not authorize tags, pushes, or publication.

For an authorized version operation:

1. Require a clean working tree and inspect `uvx changenote status` before applying the plan.
2. Run `uvx changenote version` without concurrent version commands.
3. Inspect the diff for configured version files, changelogs, `.changenote/releases.json`, and consumed notes.
4. Run `uvx changenote status` again and confirm that the applied notes are no longer pending.

`version` updates version files, prepends changelog entries, saves the latest release batch, and removes consumed notes.
With no pending notes, it changes nothing and preserves the saved batch.
It does not synchronize manifests, lockfiles, or internal dependencies.
If publication requires those updates, follow the package's documented release process; do not invent a synchronization step.
If writes fail, stop and inspect partial changes before retrying; do not automatically discard unrelated work.

For an authorized tag operation, require a clean working tree and committed release data, then run `uvx changenote tag`.
A single configured package uses `v<version>`; two or more configured packages use `changenote/<package>@v<version>`.
Tag format depends on the number of configured packages, not the number changed.
The command creates local annotated tags only.
Do not move existing tags, push, create GitHub Releases, or publish artifacts without separate authorization.

## Report the Result

Report changed note or release paths, affected packages and bumps, validation results, and any blocked action.
If no note is needed, state why.
Distinguish local version changes and local tags from remote publication.
