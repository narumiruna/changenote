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

- Review each note as one future changelog list item; the renderer supplies version headings, section headings, and the outer list marker.
  - Do not add headings or an outer list marker just to format the generated entry.
  - Use nested lists or numbered steps when needed for related details or migration instructions.
- Use a separate note and the appropriate bump for each independent change, even within one PR or with the same package and bump.
  - Update an existing note only for the same change; do not combine unrelated work from another PR.
  - Keep tightly related details together; do not split notes by sentence count or paragraph length.
- Start with a short plain-text summary of the change and its effect on users, not the implementation history.
- State required user actions and known limits when the change evidence supports them.
  - Verify current compatibility before presenting a past blocker as a current limit.
- Use active voice and one topic per sentence.
- Aim for 25 words or fewer in descriptive sentences and 20 words or fewer in instructions.
  - Use longer sentences when necessary to keep the technical meaning accurate.
- Use the same term for the same object throughout the note.
- Use simple verb forms and specific words; avoid jargon, promotional text, and vague claims.
- Put each sentence on a separate line.
  - Consecutive prose lines form one paragraph; they do not become separate changelog list items.
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

These writing defaults do not restrict supported Markdown.
Use the [combined changelog example](https://github.com/changenote/changenote/blob/main/docs/notes.md#write-for-the-generated-changelog) to check note boundaries and generated output.

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
- Use the [migration guide](https://github.com/changenote/action/blob/main/docs/migration.md) for repository moves, removed entry points, root inputs, and managed branch names.
- For an authorized upgrade, update the reviewed Action SHA, entry points, inputs, preparation scopes, and workflow contract tests together.
  - Keep the current pin's interface until the upgrade is in scope; do not apply new inputs to an older revision.

### Default to the Root Action

Use `changenote/action` and its public inputs as the default for release workflows.
First map the requested behavior to root inputs; use standalone Actions only for a concrete requirement root cannot satisfy.
Root installs the CLI and prepares supported project tools and caches by default; do not add setup or preparation Actions for a single root call.

| Requirement | Root inputs |
| --- | --- |
| Select the CLI | `version`; use `cli-path` only when a trusted same-job Setup output is already available. |
| Select the release branch | `base-branch`; empty uses the repository default branch. |
| Configure the release commit and PR | `commit-message`, `pr-title`, `pr-draft`. |
| Configure project preparation | `prepare`, `node-version`, `npm-version`, `rust-toolchain`, `uv-version`. |
| Publish Git tags or GitHub Releases | `push-git-tags`, `create-github-releases`. |
| Run project-owned build, test, or publication steps | `publish-script`, `release-script`. |
| Select the management token | `github-token`. |

Root has no `operation` input; GitHub may only warn about an undeclared input instead of failing the workflow.
With neither script nor enabled publication controls, it only manages the PR.
Do not enable publication scripts or built-in publication unless the user requests that behavior.

| `push-git-tags` | `create-github-releases` | Built-in work |
| --- | --- | --- |
| `'false'` | `'false'` | None; both controls default to this value. |
| `'true'` | `'false'` | Tags only. |
| `'true'` | `'true'` | Tags and GitHub Releases. |
| `'false'` | `'true'` | Invalid, even when tags already exist. |

Set these inputs to literal `'true'` or `'false'`.
Built-in publication creates Git tags and GitHub Releases, not registry uploads.
`published` and `published-packages` describe only this run's built-in writes, not script or registry completion.

#### Minimal Root Workflow

Start with this PR-only template when creating a release workflow.
The repository must have committed package configuration and version files.
Replace both SHA placeholders with reviewed full commit SHAs and replace `main` with the selected base branch throughout.
The template is not ready to run until those placeholders are resolved.

```yaml
name: Release
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: write
  pull-requests: write
concurrency:
  group: changenote-release-${{ github.ref }}
  cancel-in-progress: false
jobs:
  release:
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<full-checkout-commit-sha>
        with:
          fetch-depth: 0
          persist-credentials: false
      - uses: changenote/action@<full-action-commit-sha>
        with:
          base-branch: main
```

### Configure the Root Workflow

- Make requested local workflow edits without dispatching the workflow.
- Preserve the requested publication behavior.
  - Without scripts, enabled root publication controls can complete an older validated batch on later base-branch pushes.
  - Either nonblank script limits follow-up work to a qualifying new managed merge in the triggering push range.
  - Manual root invocations only manage the PR, regardless of scripts or publication controls.
  - Workflow menu choices such as `retry-publication` are caller-owned routing, not root operations; never pass them to root.
- Pin Actions to reviewed full commit SHAs from `changenote/action`, not CLI repository SHAs.
  - Pin other external Actions to reviewed full SHAs as well.
  - If a reviewed SHA is unavailable, leave a clearly marked template and report that it is not ready to run.
- Select compatible CLI and Action versions; their release versions are independent.
  - Root installs a prebuilt CLI wheel from PyPI with uv outside the consumer checkout.
  - `version` defaults to `latest`; pinning the Action SHA does not pin the CLI version.
  - For repeatable runs, use the same exact stable CLI version for PR generation and release replay.
  - Exact versions require a leading `v`; bare versions, ranges, Git revisions, URLs, and prereleases are rejected.
  - A reused `cli-path` must come from trusted same-job `/setup` output, never contributor data or another job.
  - An exact `version` must match the reused executable; `latest` with a trusted path validates its stable version without resolving PyPI again.
  - Older revisions can require caller installation or a fixed CLI source pin; follow their metadata instead of assuming current inputs exist.
  - Never build the trusted CLI from contributor code.
- Use Linux runners and restrict root to the selected base branch's push and manual events, not contributor PR events.
- Set checkout `persist-credentials: false` and caller concurrency with `cancel-in-progress: false`.
  - Concurrency is not a durable queue; newer pending runs can replace older pending runs.
- Give the root job `contents: write` and `pull-requests: write` for PR management.
  - Grant `id-token: write` only when a separately configured publication method needs OIDC.
- Record required repository settings, branch rules, and token setup for the maintainer; do not change them without authorization or request secrets in chat.
  - The repository must permit Actions to create PRs and the bot to push the managed branch.
  - When generated PRs must trigger CI, use an authorized GitHub App token or PAT through `github-token`; the default `GITHUB_TOKEN` does not trigger PR workflows.
- Check the reserved release-branch name at the selected Action revision.
  - The slash-separated format is `changenote-release/<base-branch>`; the suffix is the selected base branch or repository default, not a fixed `main`.
  - When adopting this format, update branch rules and head-branch filters, and finish publication for or close the previous Release PR.
  - Existing PRs and branches are not automatically renamed or removed; update custom managed-merge routing with the Action pin.
- Set `pr-draft` only when requested.
  - Empty creates ready PRs; `create` drafts new PRs; both preserve draft status on updates.
  - `always` also converts managed ready PRs to draft on updates, including unchanged generated trees.
- Keep changelog output enabled when root creates GitHub Releases; tags-only and script-only work can use disabled changelog output.
- Prepare consistent package-manager versions and required caches for PR generation and release replay.
  - Root defaults to `prepare: 'true'` for every invocation, including PR-only and script calls.
  - Set `prepare: 'false'` after caller-owned preparation; this does not skip required CLI or router setup.
  - Root prepares supported lockfile tools and caches, not arbitrary build dependencies.
  - Saved-batch preparation includes only consumed notes, not unrelated notes retained for the next release.

### Root Project Scripts

Keep tests, builds, and registry publication in `publish-script` when its verified-merge execution satisfies the requested workflow.
Root scripts require a new batch-changing first-parent commit and exactly one associated merged managed PR in the triggering push range.
Verify the reserved same-repository head branch, selected base, body marker, and matching merge commit; titles and commit messages are not authority.
The execution order is `publish-script`, enabled built-in tags and Releases, then `release-script`; a failure prevents later steps without undoing earlier writes.
Scripts do not enable built-in publication; keep both controls false when a script owns tags or Releases.
Scripts share a temporary worktree at the verified release commit, not the caller's checkout; earlier untracked build files are not copied.
Read `CHANGENOTE_RELEASE_COMMIT` and the per-package `CHANGENOTE_RELEASES` JSON array inside scripts; do not change HEAD.
For multiple packages, select the intended package explicitly; do not assume one global version or blindly use the first entry.
The caller supplies trusted scripts, tools, artifacts, credentials, and tests before upload.
The management token is not automatically exported to scripts; provide needed credentials explicitly through the step's `env`.
Scripts run with job authority, not in a security sandbox; never build their text from contributor data.
Metadata verification and `ready` are not proof that project CI passed or permission to publish.

### Compose Only When Root Cannot Meet the Requirement

Before introducing standalone Actions, identify the requirement root cannot satisfy and add only the entry points needed.
Do not split merely because tests or approvals are required.
Inline tests can run in a root script, and an approval that may cover the entire root job does not require splitting.
Use standalone Actions when the task requires:

1. Read-only publication inspection or independent contributor PR checks.
2. Independent CI jobs, platform-specific runners, or a separate approval between release selection and publication.
3. Different permissions or execution times for PR management and publication.
4. An independent full-publication retry, since a manual root invocation only manages PRs.

For the standalone operations below, use the [lower-level reference](https://github.com/changenote/action/blob/main/docs/github-actions-reference.md).

| Required operation | Standalone entry point |
| --- | --- |
| Run PR management independently | `changenote/action/manage-pr` |
| Inspect the verified batch without writes | `changenote/action/inspect` |
| Check contributor PR notes without writes | `changenote/action/check` |
| Publish tags and GitHub Releases for a validated batch | `changenote/action/publish` |
| Reuse one trusted CLI across necessary operations in a job | `changenote/action/setup` |
| Prepare missing project tools and offline caches for those operations | `changenote/action/prepare` |

Root plus these six standalone entry points form the current API.
The former `/release`, `/select`, and `/preview` paths are removed, not compatibility aliases.
Standalone operations install the selected CLI by default; `/setup` is optional for same-job reuse, not a mandatory first step.
Standalone `/manage-pr`, `/inspect`, and `/publish` do not prepare project tools implicitly.
Add `/prepare` only when the operation needs supported tools or caches that caller-owned setup has not already prepared.
`/prepare` uses `operation` only as an environment scope: `manage-pr` for pending notes, `inspect` or `publish` for the saved batch, and `auto` for both.
The `publish` preparation scope requires the selected `expected-release-commit`; other scopes reject that guard.
Preparation does not execute the named release operation.
Use the same CLI and project tool versions across composed jobs; do not transfer executable paths between jobs.
Inspection, preparation, and contributor checks need only `contents: read`.
Publication needs `contents: write`; PR management needs both `contents: write` and `pull-requests: write`.
Standalone `/inspect` and `/publish` always use full-publication checks and require enabled changelog output; they are not selective tags-only checks.

#### Independent CI and Publication

When a separate job must validate the release commit, use `/inspect`, CI at that commit, then `/publish` with the selected `expected-release-commit`.
Pass a nonempty full lowercase 40-character SHA to publication and its `publish` preparation scope; root accepts no such guard.
A changed latest batch must fail guarded publication, not silently switch versions.
Inspection returns the verified batch even when GitHub publication is complete.
Gate automatic GitHub work with `publication-needed`, not a nonempty `release-commit`; registry work has separate completion state.
Explicit manual downstream retries can use completed batches and must retain the selected commit guard.
Keep `/manage-pr` independent when PR-management failures must not block publication.
Select the intended package explicitly from the `releases` JSON array for multi-package downstream work.

#### Contributor Note Checks

Use the [PR check guide](https://github.com/changenote/action/blob/main/docs/checks.md) for the `/check` workflow and outputs.
Run it on `pull_request` with read-only permissions, a clean checkout, and enough history for its required `since` ref.
Do not run release management or publication on contributor PR events.
The check Action defaults to package coverage; set `coverage: pr` only when that matches repository policy.
A `skip-release` exemption requires a nonempty policy-based reason and does not bypass invalid-note validation.
Treat notes, PR titles, branch names, and other contributor data as data, not shell commands or script inputs.

### Validate and Recover

Run available repository workflow checks, including `actionlint` when available, and inspect pins, inputs, event filters, permissions, and accidental publication changes.
Read Action outputs only after a successful step, except documented failure reports from `/check` and `/inspect`.
PR-only root calls return an empty `release-commit`, `releases: '[]'`, and `ready: 'false'`; these are not a publication inspection.

Dispatch a workflow, push branches or tags, create or merge PRs, create GitHub Releases, or run publication scripts only when the user explicitly requests that specific remote action.
Permission to edit a workflow does not authorize running it or changing repository settings.
Actions never approve or merge their PRs.

After failure, inspect the selected commit, batch, remote tags, Releases, and any registry state before retrying.
Do not merge another Release PR until the current batch and downstream artifacts are complete; discovery covers only the latest batch.
For a failed root invocation, prefer re-running the original eligible run with the same inputs while it still targets the intended batch.
A new manual root invocation is PR-only, not a substitute for that retry.
Scripts repeat from the beginning even when GitHub publication is complete; they must safely handle partial and already-completed uploads.
Use standalone `/inspect` and guarded `/publish` only when a separate full-publication retry is needed.
Standalone `/publish` always handles both tags and Releases; do not use it as a selective retry for root tags-only or script-only work.
If a newer batch has replaced the intended target, stop for explicit recovery instead of repurposing the retry for that batch.
Do not move existing tags, replace published artifacts, or rewrite Release text.

## Report the Result

Report changed note, release, or workflow paths, validation results, and any blocked action.
For CLI tasks, include affected packages and bumps.
For workflow tasks, include root inputs, reviewed Action pins, CLI versions, permissions, and publication behavior.
If standalone Actions are needed, state the requirement root cannot satisfy and the necessary entry points.
Distinguish local version changes and local tags from remote publication.
For workflow tasks, list maintainer-owned setup and hosted checks that remain unverified, including token behavior, fork events, branch rules, caches, and actual publication.
Local checks do not establish hosted release readiness.
