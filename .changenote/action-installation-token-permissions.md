---
changenote: patch
---

Fix release Actions rejecting installation tokens when repository permission metadata is absent or reports push access as false. Let Git pushes and GitHub API writes check actual token access.
