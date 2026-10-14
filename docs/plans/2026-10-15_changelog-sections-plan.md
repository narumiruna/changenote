# Configurable Changelog Sections

## Goal

Add optional note types and ordered changelog sections without changing SemVer bump rules or existing default output.

## Context

`src/changelog.rs` and `scripts/action/version.sh` currently group notes by bump.
Public Rust `Config`, `Note`, and `Release` struct layouts must remain compatible.
Use `$type` in note frontmatter because package names cannot contain `$`.

## Plan

- [x] Add validated `changelog.sections` configuration and optional `$type` note metadata; preserve existing Rust APIs and default JSON.
- [x] Add `add --type` for named and generated notes, with validation before file allocation.
- [x] Add detailed status type and section output; use shared section planning for changelogs and Release PRs.
- [x] Test ordering, merged types, untyped fallback, invalid mappings, mixed bumps, fixed groups, multiline Markdown, and publication.
- [x] Update the schema, README, skill, and a minor release note.
- [ ] Run Rust and Action checks, review the diff, commit the intended paths, and open a new PR.

## Rules

Configured sections define output order.
Omit empty sections and append untyped notes under `Other Changes`.
Reject unknown types only when section configuration is present.
Reject duplicate type mappings, empty sections, empty or multiline titles, and duplicate titles, including the reserved fallback title.
Reject titles that start with a SemVer version to prevent ambiguous release headings.
Without section configuration, retain bump grouping even for typed notes.
Do not save note metadata in release batches.

## Rollback / Recovery

Upgrade CLI and Actions together before using section configuration.
Older CLIs reject the new configuration and typed frontmatter.
To stop using this feature before versioning, remove section configuration and `$type` metadata from pending notes.
Do not edit committed release batches or move release tags.

## Completion Checklist

- [x] Tests prove backward-compatible defaults and custom grouping; all Rust tests and 81 Action tests pass.
- [x] Required checks pass: Rust format, Clippy, tests, release build, Bash syntax, ShellCheck, actionlint, schema examples, and Action metadata and Python release tests.
  - Action tests initially failed because `jq` was missing; they passed with tools in `/tmp/changenote-check-tools`.
- [x] Pending status confirms the `changenote` minor bump from `0.5.1` to `0.6.0`.
- [ ] New PR contains implementation, tests, documentation, and release note.
- [ ] Delete this plan after all completion checks pass.
