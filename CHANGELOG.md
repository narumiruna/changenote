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

