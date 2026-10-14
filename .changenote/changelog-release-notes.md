---
changenote: minor
---

Use each package's exact CHANGELOG.md version entry as its GitHub Release body, preserving change groups and multiline Markdown. Add the read-only release-notes command. Breaking change: saved releases.json batches now contain only release metadata, without notes. Upgrade the CLI and Actions together. Complete old unpublished batches with matching old tooling before upgrade, or inspect and handle them manually; do not convert them by editing committed batch data. Publication validates every body before writes and retries preserve existing GitHub Release text. Nested SemVer headings remain inside their note list items, and temporary body filenames use batch indexes so long package names remain supported.
