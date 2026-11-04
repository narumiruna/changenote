---
changenote: minor
---

Support .xcconfig version files for macOS and iOS apps with byte-preserving MARKETING_VERSION updates and unchanged build numbers.

Breaking change: files with an `.xcconfig` extension now use Xcode settings instead of the plain-text SemVer format.
For an existing plain-text `.xcconfig` source, rename it to `VERSION` and update its configured path, or use `MARKETING_VERSION = <version>`.
Complete unpublished releases that use the old format with their matching CLI and Action pins before upgrading.
