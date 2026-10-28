# Change notes

Add or update a Markdown note only when the change requires a release under the repository's policy.
Decide from the effect on users and release artifacts, not only the file type.
Use package names from `config.json`, a `major`, `minor`, or `patch` bump, and a nonempty summary of the effect on users.

If no release is needed, create no note and state the reason in the completion report or PR description.
Do not invent a package bump or summary to create a note.
Empty files, whitespace-only files, delimiter-only notes, and notes without package assignments or a nonempty summary are invalid, not no-release markers.
A `check --skip-release --reason` exemption removes the missing-note requirement but still validates all pending notes.

Run `changenote status` after adding or updating notes and resolve validation errors before completion.
The Release workflow consumes notes and creates version tags and GitHub Releases.
