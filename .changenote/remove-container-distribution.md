---
changenote: minor
---

Remove the Dockerfile and the container image publication workflow. New container images are no longer published to Docker Hub or GitHub Container Registry. Use `cargo install changenote --locked` or `uvx changenote` instead.
