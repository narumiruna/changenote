# Repository instructions

## Documentation

- Follow ASD-STE100 (Simplified Technical English) for English technical prose.
  - Use the vocabulary and writing rules approved by ASD-STE100.
  - Keep required technical names.
- Use indented sub-items for rules that depend on a parent rule.
- Read `README.md` for CLI usage, workspace layout, and release behavior.

## Commands

- Run changenote commands from the repository root with `cargo run --locked -p changenote -- <command>`.

## Boundaries

- Run these release actions only when the user explicitly requests the specific action:
  - `changenote version`
  - `changenote tag`
  - The GitHub Release workflow

## Git and commits

- Add or update a note in `.changenote/` after you complete a change that requires a release.
  - Include this note before you commit or merge the change.
  - Add a note for new features, bug fixes, breaking changes, and changes to installation or other user-facing behavior.
  - No note is required for documentation-only changes or test-only changes.
  - No note is required for internal refactors that do not affect users or release artifacts.
  - Select affected package names from `.changenote/config.json`.
    - The CLI package name is `changenote`, not `cli`.
  - Select the bump type for each affected package:
    - Use `minor` for backward-compatible features.
    - Use `patch` for backward-compatible fixes.
    - Until the release compatibility policy is defined, use `minor` for breaking changes.
      - Use `major` instead only when the user explicitly requests it.
      - Describe the breaking change and migration steps in the note.
  - Create notes with `cargo run --locked -p changenote -- add <note-name> --package <package-name> --bump <major|minor|patch> --message "<release summary>"`.
    - If a note exists for the same change, update it instead of creating a duplicate.
  - Run `cargo run --locked -p changenote -- status` after you add or update notes.
    - Confirm the affected packages and bumps.
    - Resolve validation errors before you commit.
- When the user requests a commit, include the relevant note with the implementation.
- If no note is needed, state the reason in the completion report.
