---
changenote: major
---

Support optional $schema metadata in configuration and provide a repository JSON Schema for editor validation and completion.

Breaking Rust API change: Config now has a public schema: Option<String> field. Add schema: None to existing struct literals. Include schema in exhaustive destructuring, or use .. to ignore it. Existing configuration files remain supported.
