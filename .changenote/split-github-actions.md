---
changenote: minor
---

Move the root, check, preview, publish, and release GitHub Actions from `changenote/changenote` to `changenote/action`.
The CLI repository no longer provides Action entry points.
Update workflow references to the same Action paths under `changenote/action`, with a reviewed full commit SHA from that repository.
New configuration files use the schema URL under `changenote/changenote` after the repository transfer.
