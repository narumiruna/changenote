---
changenote: minor
---

Add package-level note coverage and a read-only PR report Action.

- Initialize configuration from an existing version file without changing it.
- Read note messages from a file or stdin.
- Synchronize additional version files under one release identity and support literal package tag prefixes.
- Add local doctor diagnostics without package-manager execution or network access.
- Restore known release files and consumed notes after ordinary write failures.

Upgrade CLI and Action pins together before using the new configuration fields or check Action. Release writes now reject symlink output files and parent directories. Replace these links with regular in-repository files and directories before versioning. Recovery is best-effort and does not cover process termination or concurrent writes.
