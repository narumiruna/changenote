## 2.0.0

### Major Changes

- Remove support for the `release` command.
  Use `version` to update versions and changelogs, then use `tag` to create local release tags.

### Minor Changes

- Add `status --include-note-paths` to show the source file for each pending note.

### Patch Changes

- Preserve multiline Markdown inside each changelog list item ([#123](https://github.com/owner/repo/pull/123)).
  Nested lists and code blocks keep their indentation.
