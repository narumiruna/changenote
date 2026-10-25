## 0.7.1

### Patch Changes

- Fix release publication when newer notes remain pending and prevent ordinary pushes from replacing queued publication runs.

## 0.7.0

### Minor Changes

- Add package-level note coverage and a read-only PR report Action.
  
  - Initialize configuration from an existing version file without changing it.
  - Read note messages from a file or stdin.
  - Synchronize additional version files under one release identity and support literal package tag prefixes.
  - Add local doctor diagnostics without package-manager execution or network access.
  - Restore known release files and consumed notes after ordinary write failures.
  
  Upgrade CLI and Action pins together before using the new configuration fields or check Action. Release writes now reject symlink output files and parent directories. Replace these links with regular in-repository files and directories before versioning. Recovery is best-effort and does not cover process termination or concurrent writes.

## 0.6.0

### Minor Changes

- Add optional change note types and configurable changelog sections.
  Use `add --type` and configure `changelog.sections` to group notes by type independently of SemVer bumps.
  Upgrade the CLI and Actions together before using typed notes or section configuration.

## 0.5.1

### Patch Changes

- Fix release skill guidance by placing PR links in change notes and removing instructions for unused release notes files.

## 0.5.0

### Minor Changes

- Use each package's exact CHANGELOG.md version entry as its GitHub Release body, preserving change groups and multiline Markdown. Add the read-only release-notes command. Breaking change: saved releases.json batches now contain only release metadata, without notes. Upgrade the CLI and Actions together. Complete old unpublished batches with matching old tooling before upgrade, or inspect and handle them manually; do not convert them by editing committed batch data. Publication validates every body before writes and retries preserve existing GitHub Release text. Nested SemVer headings remain inside their note list items, while indented history entries stay separate. Temporary body filenames use batch indexes so long package names remain supported, and publication passes hyphen-leading package names as option values.

## 0.4.1

### Patch Changes

- Only publish automatically after a managed Release PR merge; keep ordinary pushes limited to PR management and require an explicit operation for publication retries. Validate the selected batch again before remote writes.

## 0.4.0

### Minor Changes

- Synchronize existing Cargo.lock, uv.lock, and package-lock.json files after manifest version updates, including shared workspace locks. Install Cargo, uv, or npm and populate its cache before running version for projects with matching lockfiles. Commands run offline, npm lifecycle scripts are disabled, and manager failures restore manifest and lockfile contents without consuming notes. Upgrade the CLI and release Actions together to stage and verify lockfile outputs.

- Generate collision-safe note names when add omits the positional name.

- Support disjoint fixed version groups with shared versions and direct-note provenance.

- Add opt-in direct note paths to release plans without changing default or saved JSON.

- Add a read-only GitHub publication preview with complete batch validation and conflict reports.

### Patch Changes

- Use Cargo.toml as the repository release version source.

- Use the PYPI_TOKEN GitHub Actions secret for PyPI publication instead of Trusted Publishing. Configure a PyPI API token scoped to changenote as PYPI_TOKEN in the pypi environment or repository before publishing.

- Read the configured Cargo.toml version source when preparing Python releases and checking installed wheels, without requiring a root VERSION file. Keep build-only synchronization for older releases configured with VERSION.

## 0.3.0

### Minor Changes

- Group changelog notes by their own bump and preserve existing titles, introductions, and prior entries. Publication verifies exact current or legacy release trees for older batches.

- Create one note with independent package bumps using repeatable --major, --minor, and --patch options while retaining the shared-bump CLI and Rust add API.

- Add read-only PR note checks against a Git merge base with JSON reports and explicit release exemptions. Validate committed changes in a clean repository; this is a PR-level check, not per-package coverage.

### Patch Changes

- Replace only the top-level package.json version token, preserving all other bytes. Reject duplicate top-level version keys before writes.

## 0.2.0

- Support optional $schema metadata in configuration and provide a repository JSON Schema for editor validation and completion.
  
  Breaking Rust API change: Config now has a public schema: Option<String> field. Add schema: None to existing struct literals. Include schema in exhaustive destructuring, or use .. to ignore it. Existing configuration files remain supported.

- Support pyproject.toml, Cargo.toml, and package.json as configured version files while retaining plain-text VERSION support.

- Preserve exact numbers and objects in unrelated package.json fields when updating the version, including objects with serde_json private keys.

## 0.1.2

- Fix Linux Python release builds by creating the distribution directory with runner ownership before the manylinux container writes artifacts.

## 0.1.1

- Publish the Rust CLI to PyPI as tested Linux, macOS, and Windows wheels and a source distribution through the Release workflow using Trusted Publishing.

## 0.1.0

- Fix release Actions rejecting installation tokens when repository permission metadata is absent or reports push access as false. Let Git pushes and GitHub API writes check actual token access.

- Package the Rust CLI for the existing changenote PyPI project using a root Python package manifest and the wrapper module in python/changenote. Platform wheels include the compiled executable for use with uvx changenote, and python -m changenote runs the installed binary. The Python distribution version is read directly from the Rust CLI's Cargo.toml so the wheel metadata and CLI version match.

- Format Release PR descriptions with Changesets-style package version headings and per-note Major, Minor, and Patch Changes lists. Add status --include-note-bumps for PR generation without changing default release plans or saved batches.

- Add reusable GitHub Actions for Release PRs and retry-safe tag and GitHub Release publication, with one Release workflow for automatic runs and manual retries. Reject symlinked release outputs, including the saved release batch.

- Initial release of the Rust CLI with language-independent version files, Markdown change notes, changelogs, and single-package or monorepo release tags.

- Require Python 3.12 or later for the Python distribution; drop Python 3.10 and 3.11 support.

- Use a single root Cargo package for the Rust CLI. Update installation, Python binary packaging, CI, Docker, and release paths without changing support for monorepo release plans.

