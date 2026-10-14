---
name: changenote-manage-releases
description: Use changenote to add or update change notes, review generated CHANGELOG.md entries, inspect pending SemVer release plans, initialize release configuration, or apply versions and create tags when explicitly requested. Use for repositories with .changenote/config.json or when the user asks to adopt changenote; not for registry publication.
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

### Configuration JSON Schema

The optional `$schema` field in `.changenote/config.json` enables editor validation and completion.
`init` adds this reference:

```json
"$schema": "https://raw.githubusercontent.com/narumiruna/changenote/main/schema.json"
```

Replace `main` with a commit SHA to pin the schema, or use a local schema path.
Keep an existing schema reference unless a change is in scope.
Configurations without `$schema` remain supported.
The CLI accepts `$schema` as metadata; it does not fetch the schema or use it for validation.
Unknown configuration fields are rejected.
Run `status` to check the configuration, paths, files, and versions; editor validation does not replace CLI checks.

## Add or Update a Note

1. Identify the user-visible change and affected package names from the configuration, not directory names or guessed manifest names.
2. Follow the repository's note policy.
   - Add notes for features, fixes, breaking changes, and installation or other user-facing behavior.
   - Documentation-only changes, test-only changes, and internal refactors with no release impact normally need no note.
3. Check existing notes for the same change and update the matching note instead of adding a duplicate.
4. Use a unique note name containing only ASCII letters, digits, hyphens, or underscores, and write a concise user-facing summary.
   - Treat summaries as data and quote command arguments safely.

```sh
uvx changenote add fix-parser --package pkg-a --bump patch --message "Fix parsing of quoted package names"
```

Repeat `--package` to give several packages the same bump.
For different bumps, write or edit one note with this format.
Replace the example package names with names from the target configuration:

```markdown
---
pkg-a: patch
pkg-b: minor
---

Add support for quoted package names in the shared API.
Fix parsing of quoted package names in the CLI.
```

Only top-level `.changenote/*.md` notes are read.
Hidden files and `README.md`, `AGENTS.md`, `CLAUDE.md`, and `GEMINI.md` are ignored.
The CLI rejects unknown packages, invalid bumps, duplicate YAML keys, and empty summaries; `add` does not overwrite an existing note.

Run `uvx changenote status` after adding or updating notes.
Confirm that its JSON plan contains the intended packages, bumps, versions, and summaries, and resolve validation errors before completion.
`status` does not change files.
For a note-only task, stop after validation; do not consume the notes.

## Write Change Notes

Use ASD-STE100 principles for English note text; full compliance is not required.
Give priority to technical accuracy and clear meaning.
Keep required technical names and terms, and use conventional release-note verbs such as `Add`, `Fix`, and `Remove`.

- Describe the change and its effect on users, not the implementation history.
- Use active voice and one topic per sentence.
- Aim for 25 words or fewer in descriptive sentences and 20 words or fewer in instructions.
  - Use longer sentences when necessary to keep the technical meaning accurate.
- Use the same term for the same object throughout the note.
- Use simple verb forms and specific words; avoid jargon, promotional text, and vague claims.
- Put each sentence on a separate line.
- Write commands, paths, option names, and API names in backticks.
- For a breaking change, identify what no longer works and give the required migration steps.
  - Use imperative sentences for instructions.
  - Select the bump from the repository's compatibility policy; do not assume every breaking change requires `major`.
- Check the technical facts and English text before completion.
  - `status` checks note structure and the release plan; it does not check ASD-STE100 compliance.

### Fix Example

Example `.changenote/fix-quoted-names.md`:

```markdown
---
pkg-a: patch
---

Fix parsing of quoted package names in change notes.
The CLI now accepts names such as `"pkg-a"`.
```

### Breaking Change Example

Example `.changenote/remove-release-command.md` for a repository that uses `minor` for breaking changes:

```markdown
---
pkg-a: minor
---

Remove the `release` command.
Existing scripts that use this command no longer work.

To update your scripts:

1. Replace `changenote release` with `changenote version` to update versions and changelogs.
2. Use `changenote tag` separately to create local release tags.
```

Use only examples that match the actual change.
Replace `pkg-a` with the configured package name.

## Changelog and Release Notes Format

Omit empty groups.
Each package has its own `CHANGELOG.md` in its configured directory.
Group notes by each note's bump for that package, not by the package's overall release bump.
Include migration steps for breaking changes and keep multiline Markdown inside each note's list item.
To include a PR number, add a Markdown link to the pending note summary, such as `Fix output ([#123](https://github.com/owner/repo/pull/123))`.
Use the actual repository URL and a verified PR number; omit the reference if the PR is not known.
`version` preserves this link in the changelog, and GitHub Release publication preserves it in the release body.
changenote does not look up or add PR numbers automatically.
Use pending change notes as input to `version`; do not create a separate `RELEASE_NOTES.md`.

GitHub Release publication reads the exact version entry from each package's changelog in the validated release commit, without its version heading.
Saved `.changenote/releases.json` contains only package, old version, version, bump, and tag metadata; pending status still includes notes.
Publication rejects manual edits to the generated release tree.
Missing, empty, or duplicate version entries stop publication, and retries do not replace existing GitHub Release text.
Upgrade the CLI and Actions together; old saved batches with `notes` are unsupported and need completion with matching old tooling or manual handling before upgrade.
Do not convert old batches by editing committed data.

## Apply Versions or Create Tags

Run `version`, `tag`, or a GitHub Release workflow only when the user explicitly requests that specific release action.
Permission to add notes does not authorize versioning; permission to version does not authorize tags, pushes, or publication.

For an authorized version operation:

1. Require a clean working tree and inspect `uvx changenote status` before applying the plan.
2. Run `uvx changenote version` without concurrent version commands.
3. Inspect the generated diff and confirm that it contains only the intended release changes.
4. Run `uvx changenote status` again and confirm that the applied notes are no longer pending.

`version` updates version files, prepends changelog entries, saves the latest release batch, and removes consumed notes.
With no pending notes, it changes nothing and preserves the saved batch.
It updates configured version files and synchronizes supported existing lockfiles; it does not update internal dependency ranges.
Follow the repository's documented package-manager and offline-cache requirements.
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
