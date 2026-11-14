# Lower-level GitHub Actions

Use these Actions when the workflow manages CLI installation and package-manager setup.
For automatic setup, use the [config-driven release Action](github-actions.md#config-driven-github-publication).
For project-owned publication scripts, use the [root Action guide](github-actions.md#root-action-post-merge-commands).

- [Requirements](#requirements)
- [Release PR workflow](#release-pr-workflow)
- [Publication preview](#publication-preview)
- [Publication workflow](#publication-workflow)
- [Inputs and outputs](#inputs-and-outputs)
- [Recovery and limits](#recovery-and-limits)

For contributor PRs, use the read-only [check Action](checks.md#github-action-report).

## Requirements

The Release PR and publication Actions require Linux, Bash, Git, `gh`, `jq`, a clean checkout, and an installed compatible `changenote` executable on `PATH`.
Ubuntu GitHub-hosted runners include the non-CLI tools.
These Actions do not install the CLI.

- Run at the repository root.
  - Action scripts come from the Action checkout, not the consumer repository.
- Package version files, lockfiles, changelogs, and `.changenote/releases.json` must not be symlinks.
  - Both writing Actions check the batch path before CLI writes.
- Prepare the package managers and cached dependencies required by [lockfile synchronization](configuration.md#automatic-lockfile-synchronization).
  - The CLI updates configured primary and additional version files and supported existing lockfiles, not unlisted manifests.
- Upgrade CLI and Action pins together.
  - Use reviewed compatible full commit SHAs.

## Release PR workflow

This two-job example uses the root Action for PR management and `/publish` for built-in tags and GitHub Releases.
It does not use the project script inputs. Use the [command-hook workflow](../examples/github-actions/command-hooks.yml) when your project owns publication.
Replace each SHA placeholder with a reviewed full commit SHA. The CLI and Action must use compatible release-plan formats.

```yaml
name: Release
on:
  push:
    branches: [main]
  workflow_dispatch:
permissions:
  contents: read
concurrency:
  group: changenote-release-main
  cancel-in-progress: false
jobs:
  release:
    permissions:
      contents: write
      pull-requests: write
    if: github.ref == 'refs/heads/main'
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote@<full-action-commit-sha>
        with:
          base-branch: main

  publish:
    needs: release
    if: ${{ always() && github.ref == 'refs/heads/main' && needs.release.result != 'cancelled' }}
    permissions:
      contents: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          ref: main
          fetch-depth: 0
          persist-credentials: false
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote/publish@<full-action-commit-sha>
        with:
          base-branch: main
```

The Action fetches the current base branch, applies pending notes in a temporary worktree, and creates or updates one PR from `narumi/chore/changenote-release-<base-branch>`. Reserve that branch for automation. New notes update the same PR. A rerun creates no commit or push when the release branch includes the current base commit and its generated tree is unchanged. If notes are removed, the Action closes only its managed PR and leaves the branch in place. A manually closed PR is replaced when notes remain.

Release PR descriptions use a `Releases` heading and `package@version` headings. By default, notes are grouped by their own bump under `Major Changes`, `Minor Changes`, and `Patch Changes`; empty groups are omitted. Multiline notes remain within their list items. The Action requires a CLI that supports `status --include-note-sections`. This option adds ordered `note_sections` objects with `title` and `notes`; default release-plan output and saved batches are unchanged. Custom section configuration replaces the bump groups for both Release PRs and changelogs.

Existing branches require a marked PR in their history. Lease-based pushes stop if another actor moves the branch. The Action stages only release outputs and consumed notes. Notes and titles are data, not shell commands. PR bodies over 60,000 bytes omit release details.

In repository settings, enable **Actions > General > Allow GitHub Actions to create and approve pull requests**. Branch rules must permit the bot to push the release branch. The Action never approves or merges its PR.

PRs created with the default `GITHUB_TOKEN` do not automatically trigger PR workflows. If CI must run on the Release PR, use a GitHub App token (recommended) or PAT with repository contents and pull requests write permissions. Pass it through `github-token`. Checkout credentials do not select the Action's token.

## Publication preview

The preview Action requires Linux, Bash, Git, `gh`, `jq`, an installed compatible CLI, and standard coreutils. It needs only `contents: read`. It reads the selected remote branch in a temporary repository; the caller's files, index, refs, tags, and Git identity are unchanged. A dirty or stale source checkout does not select the release. Temporary fetches and release-tree reconstruction are removed on success and failure.

```yaml
name: Publication preview
on:
  workflow_dispatch:
permissions:
  contents: read
jobs:
  preview:
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@stable
      - name: Install changenote
        run: cargo install --git https://github.com/narumiruna/changenote --rev <full-cli-commit-sha> --locked
      - uses: narumiruna/changenote/preview@<full-action-commit-sha>
        with:
          base-branch: main
```

This is a template, not part of the production release workflow. Use the same reviewed compatible full SHA for CLI and Action. Preview uses the same latest-batch discovery, full-tree verification, and remote-state inspection as publication. It never creates tags, commits, PRs, or Releases and does not inspect registries.

The `publication-plan` output is a compact JSON array of `name`, `version`, `tag`, `action`, and `conflicts`. Actions are `create-tag-and-release`, `create-release`, `none`, or `conflict`. Conflicts include a wrong remote tag identity, a draft Release, or a Release without its remote tag. Every batch entry is inspected before a conflict report is emitted. The `ready` output is true only after complete inspection finds no conflicts. `release-commit` identifies the verified batch commit.

When no batch exists, the Action succeeds with `ready=true`, an empty commit, and `[]`. Conflicts return the complete inspected report with `ready=false` and fail the step. Invalid input, tree/batch validation failures, or unavailable remote data fail without a ready plan; use the error diagnostic, not an empty plan, as the result. A readable summary goes to the step log. API failures are never treated as missing releases.

The preview is advisory, not a reservation. Remote state can change afterward; real publication repeats preflight and race checks. Preview does not grant permission to publish.

## Publication workflow

The two-job workflow above runs publication after PR management on every base-branch push.
Publication can recover an earlier batch even if PR management fails.
Its job needs only `contents: write`.

In this example, manual dispatch runs both PR management and built-in GitHub publication again.
This differs from the root script workflow, where manual dispatch only manages the PR.
The [changenote repository's release workflow](development.md#repository-release-workflow) uses its own routing policy.

### Release-tree verification

Publication selects the latest first-parent commit that changes `.changenote/releases.json`. It reconstructs that commit from the parent's consumed notes and compares the complete generated tree before publication. Parent notes that remain in the release commit are excluded from replay and restored unchanged before tree comparison. Replay preserves note filename bytes without UTF-8 conversion. A Release PR can therefore leave notes that reached the base branch after its generation pending for the next release. Unrelated edits, including changes to retained notes, still fail tree verification. The publication Action checks current output, then output without lockfile synchronization. For historical note-bearing batches, it also checks the older flat-changelog profile. Historical batch bytes are regenerated from the validated parent plan in a temporary worktree. Mixed output profiles and arbitrary edits are rejected. The hidden `version --no-lockfiles` option is reserved for reconstruction in a temporary Action worktree; do not use it for new releases. Standard merge, squash, and rebase merges are supported. A later unrelated commit or new pending notes do not change the release target.

### GitHub Release bodies

Each GitHub Release body comes from the corresponding package's `CHANGELOG.md` entry in the validated release commit. The package's configured directory selects the file, and the saved version selects the entry. Publication removes the version heading but preserves the Markdown body, including bump groups, nested lists, links, and code blocks. Empty groups are absent because changelog generation omits them. All package bodies are checked before any tag or Release write. Missing files, missing or duplicate version entries, and empty bodies stop publication without a saved-notes fallback. Later changelog edits do not change the selected body. Fixed-group members without direct notes keep their changelog's explanatory message.

Inspect entries locally with [`release-notes`](releases.md#read-a-release-entry).

### Saved-batch compatibility

Upgrade the CLI and all Action pins together. New releases and local tag creation require metadata-only saved batches. The Actions accept historical batches containing `notes` only after full batch and tree verification confirms a known writer profile, every remote tag points to the release commit, and every GitHub Release exists without draft status. A completed historical batch is a successful no-op, including explicit publication retries. Missing or conflicting historical publication stops without remote writes; complete it with matching old tooling or inspect it manually. Do not edit committed batch data to convert it: full-tree verification rejects manual changes. The next authorized `version` operation writes the new format. Existing GitHub Release text is never rewritten.

### Remote state

Existing remote tags must point to the selected commit. Missing tags and GitHub Releases are created; existing release text is not replaced. Draft releases, missing tags for existing releases, malformed batches, batch deletion, manually edited release trees, and tag conflicts stop publication. A batch in a root commit cannot be verified.
If no batch exists, publication makes no changes.

The built-in `/release` and `/publish` Actions can recover publication of the latest validated batch when invoked after later pushes. They do not implement registry uploads.
The root Action runs custom scripts only for a managed batch merged in the triggering push range.
An unrelated later push does not retry an older batch, and manual dispatch only manages the PR.
To retry custom scripts, re-run the original eligible workflow. A newer batch blocks that retry; scripts must handle partial publication safely.

## Inputs and outputs

| Action | Input | Default |
| --- | --- | --- |
| Release PR, Publication, Preview | `github-token` | Workflow token |
| Release PR, Publication, Preview | `base-branch` | Repository default branch |
| Release PR | `commit-message` | `chore(release): version packages` |
| Release PR | `pr-title` | `chore(release): version packages` |
| Root | `publish-script`, `release-script` | Empty; no custom publication |
| Publication | `expected-release-commit` | Empty; retain latest-batch selection |

The root Action outputs `has-notes` (`true` or `false`), `pr-number` (number or empty string), and `pr-operation` (`created`, `updated`, `closed`, or `none`). `updated` includes a no-op version rerun that refreshes the PR metadata.
It also returns `release-commit` and `releases` for a verified managed merge, independently of configured commands or GitHub publication state. No selected merge returns an empty commit and `[]`. Read these outputs only after success.

The publication Action accepts an optional `expected-release-commit`, a full lowercase 40-character commit SHA.
A mismatch with the verified latest batch fails before remote tag or Release writes.
Use this input after external CI instead of calling internal scripts.
Both preview and publication output `releases`, a JSON array of `package`, `version`, and `tag` for the verified batch.
It includes all batch packages, including retries that create no remote objects.
If no batch exists, `releases` is `[]`.

The publication Action outputs `release-commit` (commit SHA or empty string), `published` (`true` if this run created any remote tag or GitHub Release), and `published-packages` (JSON, for example `[{"name":"pkg-a","version":"1.2.0"}]`). This is not registry publication. Read outputs only after successful steps.

## Recovery and limits

Disable the Release workflow to stop automation. Unmerged Release PR changes do not affect the base branch.

After a partial failure, retry the latest validated batch before merging another Release PR.
Discovery covers only the latest batch.
History rewrites or manual batch edits require manual inspection.
Never move tags automatically.
For this repository's retry procedure, see [Repository release workflow](development.md#repository-release-workflow).

If PR creation fails after pushing a new branch, the Action checks for a successful remote PR creation before it removes that branch with a lease. If the API or cleanup also fails, inspect the marked PR and branch manually before retrying; an unowned branch is deliberately rejected. Do not delete a branch without checking its commit and PR history.

Before production use, verify repository permissions, branch rules, and App-token CI behavior in a disposable repository.
