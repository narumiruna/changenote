---
changenote: minor
---

Package the Rust CLI for the existing changenote PyPI project using a root Python package manifest and the wrapper module in python/changenote. Platform wheels include the compiled executable for use with uvx changenote, and python -m changenote runs the installed binary. The Python distribution version is read directly from the Rust CLI's Cargo.toml so the wheel metadata and CLI version match.
