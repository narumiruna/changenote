---
name: changenote
description: Manage change notes and releases with the changenote CLI and changenote/action GitHub Actions. Use to initialize .changenote/config.json, add or update notes, inspect SemVer plans and changelogs, apply versions or local tags when explicitly requested, or set up, review, migrate, and troubleshoot release workflows, contributor checks, CI, and publication recovery. Not for standalone registry publication.
---

# changenote

## Inspect the Repository

Read the target repository's instructions and release documentation.
For CLI tasks, inspect `.changenote/config.json`, pending notes, and configured version files.
For workflow tasks, also inspect the workflow and release state described under [GitHub Actions](#github-actions).

## CLI Operations

### Configure the CLI

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

#### Configuration JSON Schema

The optional `$schema` field in `.changenote/config.json` enables editor validation and completion.
`init` adds this reference:

```json
"$schema": "https://raw.githubusercontent.com/changenote/changenote/main/schema.json"
```

Replace `main` with a commit SHA to pin the schema, or use a local schema path.
Keep an existing schema reference unless a change is in scope.
Configurations without `$schema` remain supported.
The CLI accepts `$schema` as metadata; it does not fetch the schema or use it for validation.
Unknown configuration fields are rejected.
Run `status` to check the configuration, paths, files, and versions; editor validation does not replace CLI checks.

### Add or Update a Note

1. Decide whether the change requires a release under the repository's note policy, based on its effect on users and release artifacts, not only its file type.
   - Add notes for features, fixes, breaking changes, and installation or other user-facing behavior.
   - Documentation-only changes, test-only changes, and internal refactors with no release impact normally need no note.
   - If no release is needed, stop note creation and report why; do not create an empty note or invent a bump or summary.
   - If the repository's policy conflicts with these note requirements, report the conflict instead of changing the policy or creating an invalid note.
2. For a release change, identify affected package names from the configuration, not directory names or guessed manifest names.
3. Check existing notes for the same change and update the matching note instead of adding a duplicate.
4. Use `add` for a new note with a valid package bump and a concise, nonempty user-facing summary.
   - For an explicit name, use only ASCII letters, digits, hyphens, or underscores.
   - Treat summaries as data and quote command arguments safely.

```sh
uvx changenote add fix-parser --package pkg-a --bump patch --message "Fix parsing of quoted package names"
```

If `.changenote/config.json` defines `changelog.sections`, select a configured type and pass `--type <type>` to `add`.
Select the SemVer bump separately; a type does not determine release compatibility.
For directly edited notes, store the type as `$type: <type>` in frontmatter, alongside package bumps.
A note has one type for all its packages; untyped notes remain valid.
Use `status --include-note-types --include-note-sections` when you need to verify types and planned grouping.
Do not change section configuration unless configuration changes are in scope.

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
Empty files, whitespace-only files, delimiter-only notes, and notes without package assignments are also invalid, not no-release markers.
Preserve unrelated pending notes.
Report invalid existing notes; do not delete them or use `check --skip-release --reason` to hide validation errors.
A check exemption removes the missing-note requirement, not validation of pending notes.

Run `uvx changenote status` after adding or updating notes.
Confirm that its JSON plan contains the intended packages, bumps, versions, and summaries, and resolve validation errors before completion.
`status` does not change files.
For a note-only task, stop after validation; do not consume the notes.

### Write Change Notes

Use ASD-STE100 principles for English note text; full compliance is not required.
Give priority to technical accuracy and clear meaning.
Keep required technical names and terms, and use conventional release-note verbs such as `Add`, `Fix`, and `Remove`.

- Use a separate note for each independent change, even when the package and bump are the same.
  - Update an existing note only for the same change; do not combine unrelated work from another PR.
- Start with a short summary of the change and its effect on users, not the implementation history.
- State required user actions and known limits when the change evidence supports them.
  - Verify current compatibility before presenting a past blocker as a current limit.
- Use active voice and one topic per sentence.
- Aim for 25 words or fewer in descriptive sentences and 20 words or fewer in instructions.
  - Use longer sentences when necessary to keep the technical meaning accurate.
- Use the same term for the same object throughout the note.
- Use simple verb forms and specific words; avoid jargon, promotional text, and vague claims.
- Put each sentence on a separate line.
- Write commands, paths, option names, and API names in backticks.
- To include a PR reference, add a Markdown link to the note summary, such as `Fix output ([#123](https://github.com/owner/repo/pull/123))`.
  - Use the actual repository URL and verified PR numbers; preserve several links when several PRs explain the change.
  - Omit unknown references; do not guess a PR number or substitute the release PR for the implementation PR.
  - changenote does not look up or add PR numbers automatically.
- If `changelog.source_repository` is configured, generated sections also contain automatic commit links from local note history.
  - These links identify note sources, not necessarily the complete implementation.
  - Commit new or changed notes before an authorized version operation and provide complete Git history.
  - Before commit, status can show the summary without automatic links; this does not prove release readiness.
  - Do not change configuration just to add a note. See the [source-link rules](https://github.com/changenote/changenote/blob/main/docs/notes.md#automatic-commit-source-links).
- For a breaking change, identify what no longer works and give the required migration steps.
  - Use imperative sentences for instructions.
  - Select the bump from the repository's compatibility policy; do not assume every breaking change requires `major`.
- Check the technical facts and English text before completion.
  - `status` checks note structure and the release plan; it does not check ASD-STE100 compliance.

#### Fix Example

Example `.changenote/fix-quoted-names.md`:

```markdown
---
pkg-a: patch
---

Fix parsing of quoted package names in change notes.
The CLI now accepts names such as `"pkg-a"`.
```

#### Breaking Change Example

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

### Apply Versions or Create Tags

Run `version` or `tag` only when the user explicitly requests that specific release action.
Permission to add notes does not authorize versioning; permission to version does not authorize tags, pushes, or publication.

Use pending change notes as input to `version`.
Do not manually edit the generated release tree; publication validates it.
Complete old saved batches containing `notes` with matching old tooling, or inspect release state before manual handling.
Do not convert old batches by editing committed data.

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

## GitHub Actions

### Inspect the Workflow

- Read `.github/workflows/`, `.changenote/config.json`, and saved release state.
- Identify the selected base branch, existing Action pins and CLI versions, package-manager requirements, and publication policy.
- Use the [GitHub Actions guide](https://github.com/changenote/action/blob/main/docs/github-actions.md) for setup and operations.
- Read documentation and Action metadata at the selected revision before changing inputs; `main` can differ from a pinned revision.
- Use the [migration guide](https://github.com/changenote/action/blob/main/docs/migration.md) when replacing old `changenote/changenote` Action paths.

### Select the Entry Point

| Goal | Entry point | CLI setup |
| --- | --- | --- |
| Configure the complete release workflow or verified project scripts | `changenote/action` | Installs or reuses the CLI; built-in operations prepare project tools and caches. |
| Install one trusted CLI for reuse within a job | `changenote/action/setup` | Returns `cli-path` and the exact installed `version`. |
| Prepare project tools and offline lockfile caches only | `changenote/action/prepare` | Installs or reuses the CLI; creates no PRs, tags, or Releases. |
| Manage the Release PR only | `changenote/action/manage-pr` | Installs or reuses the CLI; caller prepares project tools. |
| Return a verified batch and its GitHub publication state | `changenote/action/select` | Installs or reuses the CLI; caller prepares project tools. |
| Check contributor PR notes without writes | `changenote/action/check` | Installs or reuses the CLI. |
| Publish tags and GitHub Releases for a validated batch | `changenote/action/publish` | Installs or reuses the CLI; caller prepares project tools. |
| Keep an existing built-in release workflow | `changenote/action/release` | Compatibility entry point for root's built-in operations. |
| Keep an existing publication preview | `changenote/action/preview` | Compatibility name for the same inspection as `/select`. |

Use the [lower-level reference](https://github.com/changenote/action/blob/main/docs/github-actions-reference.md) for separate operations and recovery.
Root defaults to `operation: hooks`, preserving managed-merge scripts without built-in publication.
In `hooks`, the root Action does not publish when both `publish-script` and `release-script` are absent.
Explicitly choose `operation: auto` for root's built-in PR management and GitHub publication.
Nonempty script inputs are rejected outside `hooks`; use separate Actions when CI or approvals divide the stages.
With both scripts, `publish-script` runs first; a failure prevents `release-script`.
The `/release` Action has no script inputs.
Built-in publication creates Git tags and GitHub Releases, not registry uploads.

### Configure the Workflow

- Make requested local workflow edits without dispatching the workflow.
- Do not enable automatic publication unless the user requests it.
  - Use `/manage-pr` or root with `operation: manage-pr` for PR-only setup.
  - Root defaults to `hooks`; its `auto` operation is explicit.
  - The `/release` compatibility path still defaults to `auto`.
  - `auto` can publish the latest incomplete validated batch on base-branch pushes, even after unrelated later commits.
  - On manual events, `auto` only manages the PR; `retry-publication` explicitly retries built-in publication.
- Pin Actions to reviewed full commit SHAs from `changenote/action`, not CLI repository SHAs.
  - Pin other external Actions to reviewed full SHAs as well.
  - If a reviewed SHA is unavailable, leave a clearly marked template and report that it is not ready to run.
- Select compatible CLI and Action versions when upgrading release tooling; their release versions are independent.
  - All current entry points accept `version`; standalone operation Actions install a prebuilt CLI wheel from PyPI with uv outside the consumer checkout.
  - To reuse one installation, pass the trusted same-job `cli-path` from `/setup`; do not accept an executable path from contributor data.
  - An exact `version` must match the reused CLI.
  - With `latest` and an explicit path, reuse validates the installed stable version without resolving PyPI again.
  - Do not transfer executable paths between jobs.
  - `version` defaults to `latest`, the latest stable published CLI; pinning the Action SHA does not pin this version.
  - For repeatable runs, use the same exact stable CLI version across PR generation, selection, and publication.
  - Exact versions require a leading `v`; bare versions, ranges, Git revisions, URLs, and prereleases are rejected.
  - Older Action revisions can require caller installation or use a fixed CLI source pin; follow their metadata instead of assuming `version` exists.
  - Never build the CLI from contributor code.
- Use Linux runners and restrict release management and publication to the selected base branch's push and manual events.
- Set checkout `persist-credentials: false` and configure caller concurrency with `cancel-in-progress: false`.
  - Concurrency is not a durable queue; newer pending runs can replace older pending runs.
- Give each job only the permissions it needs.
  - PR management needs `contents: write` and `pull-requests: write`.
  - Selection, preview, and contributor checks need only `contents: read`.
  - Built-in tag and GitHub Release publication needs `contents: write`.
  - Grant `id-token: write` only when a separately configured publication method needs OIDC.
- Record required repository settings, branch-rule changes, and token setup for the maintainer; do not change them without authorization or request secrets in chat.
  - The repository must permit Actions to create PRs and the bot to push the managed branch.
  - When generated PRs must trigger CI, use an authorized GitHub App token or PAT through `github-token`; the default `GITHUB_TOKEN` does not trigger PR workflows.
- Keep changelog output enabled for built-in publication.
- Prepare the same package-manager versions and required caches for PR generation and release replay.
  - Root's built-in operations and `/release` prepare supported lockfile caches unless `prepare: 'false'` selects caller-owned preparation.
  - For composed workflows, run `/prepare` with the matching environment scope before `/manage-pr`, `/select`, or `/publish`.
  - The `hooks` operation retains caller-owned project setup.
  - Preparation for a saved batch includes only consumed notes, not unrelated notes retained for the next release.

#### Contributor Note Checks

Use the [PR check guide](https://github.com/changenote/action/blob/main/docs/checks.md) for the `/check` workflow and outputs.
Run it on `pull_request` with read-only permissions, a clean checkout, and enough history for its required `since` ref.
Do not run release management or publication on contributor PR events.
The check Action defaults to package coverage; set `coverage: pr` only when that matches repository policy.
A `skip-release` exemption requires a nonempty policy-based reason and does not bypass invalid-note validation.
Treat notes, PR titles, branch names, and other contributor data as data, not shell commands or script inputs.

#### External CI and Project Scripts

When CI must validate the exact release commit, use `/select`, external CI, then `/publish` with the selected `expected-release-commit`.
Root and `/release` expose equivalent built-in operations when a composed workflow is not needed.
Selection always returns the verified batch, including completed GitHub publication.
Gate automatic GitHub work with `publication-needed`, not the presence of `release-commit`; registry work has separate completion state.
Keep `/manage-pr` independent when PR-management failures must not block publication.
Do not use `auto` as proof that project CI passed.
For multiple packages, select the package explicitly from the `releases` JSON array; do not assume one global version or use its first entry blindly.

In `operation: hooks`, the root Action runs configured scripts only for a verified managed release merge in the triggering push range.
Manual dispatch manages its PR only; it does not run those scripts.
Scripts run at the verified release commit in a temporary worktree, not the caller's checkout; earlier untracked build files are not copied.
The caller supplies trusted scripts, tools, artifacts, credentials, and tests before upload.
Scripts must handle repeated execution and partial publication safely.

### Validate and Recover

Run available repository workflow checks, including `actionlint` when available, and inspect the diff for pins, inputs, event filters, permissions, and accidental publication changes.
For an authorized read-only hosted inspection, use `/select`, its `/preview` compatibility name, or root with `operation: select`.
A successful preview does not authorize publication or prove CI success; read Action outputs only after a successful step.

Dispatch a workflow, push branches or tags, create or merge PRs, create GitHub Releases, or run publication scripts only when the user explicitly requests that specific remote action.
Permission to edit a workflow does not authorize running it or changing repository settings.
Actions never approve or merge their PRs.

After failure, inspect the selected commit, batch, remote tags, Releases, and any registry state before retrying.
Do not merge another release PR until the current batch and downstream artifacts are complete; discovery covers only the latest batch.
Retry root scripts by re-running the original eligible workflow, not by an unrelated push or manual dispatch.
Do not move existing tags, replace published artifacts, or rewrite Release text.

## Report the Result

Report changed note, release, or workflow paths, validation results, and any blocked action.
For CLI tasks, include affected packages and bumps.
For workflow tasks, include entry points, reviewed Action pins, CLI versions, permissions, and publication behavior.
Distinguish local version changes and local tags from remote publication.
For workflow tasks, list maintainer-owned setup and hosted checks that remain unverified, including token behavior, fork events, branch rules, caches, and actual publication.
Local checks do not establish hosted release readiness.
