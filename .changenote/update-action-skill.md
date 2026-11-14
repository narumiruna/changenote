---
changenote: patch
---

Fix the bundled skill guidance to prefer `changenote/action` and its inputs for release workflows.
Use standalone Actions only for requirements that root cannot satisfy, and add only the necessary entry points.
Document PR-only defaults, explicit publication controls, and guarded independent CI and publication.
Require durable routing or a recovery path before enabling scripts that can be skipped when a pending run is replaced.
