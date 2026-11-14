---
changenote: minor
---

Run optional publish-command and release-command scripts after a verified managed version PR merge. Run publish before release, with both commands disabled by default.

The root Action now accepts only base-branch push and workflow_dispatch events. Move contributor PR note checks to the /check Action. Keep the installed compatible CLI and lockfile tools; the root Action sets up uv for merge routing. Existing /release, /publish, /preview, and /check contracts remain supported.

Commands run at the verified commit with batch metadata and caller-provided credentials. Manual dispatch manages the PR only. Re-running an eligible workflow repeats commands; scripts must handle partial publication safely.
