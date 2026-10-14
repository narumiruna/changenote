---
changenote: minor
---

Synchronize existing Cargo.lock, uv.lock, and package-lock.json files after manifest version updates, including shared workspace locks. Install Cargo, uv, or npm and populate its cache before running version for projects with matching lockfiles. Commands run offline, npm lifecycle scripts are disabled, and manager failures restore manifest and lockfile contents without consuming notes. Upgrade the CLI and release Actions together to stage and verify lockfile outputs.
