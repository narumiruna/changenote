# Repository instructions

## Documentation

- Read `README.md` for CLI usage, workspace layout, and release behavior.

## Commands

- Run changenote commands from the repository root with `cargo run --locked -p changenote -- <command>`.

## Boundaries

- Run `changenote version`, `changenote tag`, or the GitHub Release workflow only when the user explicitly requests the corresponding release action.

## Git and commits

- After completing a change that requires a release, add or update a note in `.changenote/` before committing or merging the change.
- Add a note for new features, bug fixes, breaking changes, and changes to installation or other user-facing behavior.
- Do not require a note for documentation-only changes, test-only changes, or internal refactors that do not affect users or release artifacts.
- Select affected package names from `.changenote/config.json`; the CLI package name is `changenote`, not `cli`.
- Use `minor` for backward-compatible features, `patch` for backward-compatible fixes, and `major` for breaking changes.
- Create notes with `cargo run --locked -p changenote -- add <note-name> --package <package-name> --bump <major|minor|patch> --message "<release summary>"`, or update an existing note for the same change without duplicating it.
- Run `cargo run --locked -p changenote -- status` after adding or updating notes, confirm the affected packages and bumps, and resolve validation errors before committing.
- Include the relevant note with the implementation when a commit is requested; if no note is needed, state the reason in the completion report.
