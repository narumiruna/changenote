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

- Decide whether a change requires a release from its effect on users and release artifacts, not only its file type.
  - If no release is needed, create no note and state the reason in the completion report.
  - Do not create empty or incomplete notes as no-release markers or invent a package bump or summary.
- Add or update a note in `.changenote/` after you complete a change that requires a release.
  - Include this note before you commit or merge the change.
  - Add a note for new features, bug fixes, breaking changes, and changes to installation or other user-facing behavior.
  - For changes in `skills/`, decide whether a note is required from the effect on agent behavior, not the file type.
    - Add a note when a change modifies a skill's workflow, limits, or default behavior.
    - No note is required for text or structure changes that preserve the skill's operating instructions.
  - No note is required for other documentation-only changes or test-only changes.
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
    - Confirm the affected packages, bumps, and nonempty summaries.
    - Resolve validation errors before you commit.
    - Report invalid existing notes; do not delete unrelated notes or use a check exemption to hide validation errors.
- When the user requests a commit, include the relevant note with the implementation.
