---
changenote: minor
---

Move the Rust CLI into a single-member Cargo workspace while preserving Cargo and Python distribution builds.
To install from a source checkout, use `cargo install --locked --path crates/changenote`; registry installation commands are unchanged.
The source skill moves from `skills/changenote/SKILL.md` to `crates/changenote/SKILL.md`.
Update direct source-path references, or run `changenote skill` to retrieve the bundled Markdown without a source checkout.
