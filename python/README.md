# changenote Python distribution

Distribute the Rust changenote CLI as a Python package. The package uses [maturin](https://www.maturin.rs/) with `bindings = "bin"`, like [uv's Python distribution](https://github.com/astral-sh/uv/tree/main/python/uv).

The PyPI distribution, executable, and Python module names are `changenote`. The module is in `python/changenote`, with package metadata in root `pyproject.toml`; Python does not use a uv workspace. Python wrapper changes use the `changenote` release name in `.changenote/config.json`; the wrapper has no independent version file or release plan. CLI logic stays in root `src/`.

## Run with uvx

After this binary-wrapper version is published to the existing [changenote PyPI project](https://pypi.org/project/changenote/) with a wheel for your platform:

```sh
uvx changenote --help
uvx changenote status
```

This binary-wrapper version has not yet been uploaded; existing PyPI releases are unchanged. To test a local wheel, run from the repository root:

```sh
uv build --out-dir dist/python-binary
uvx --isolated --no-index --find-links dist/python-binary changenote --help
```

A wheel contains the compiled Rust executable and does not require a Rust toolchain at runtime. A source installation requires Rust and Cargo. Each wheel supports its target platform; a local build does not produce wheels for all platforms.

## Development

Run from the repository root:

```sh
uv sync --locked
uv run --locked changenote status
uv run --locked python -m changenote --help
uv run --locked python -m unittest discover -s python/tests
```

The optional Python module runner finds the installed binary and passes arguments, the working directory, and the environment to it. It preserves the executable's exit code. It does not download a binary or use a system executable from `PATH`.

uv cache keys include the Rust sources and Cargo manifests, so Rust edits cause local package rebuilds. `uv build` produces a source distribution and builds a wheel from it, including the Rust package sources.

The Python distribution uses `dynamic = ["version"]`: maturin reads the version directly from root `Cargo.toml`. The wheel metadata and the packaged CLI's `--version` therefore use the same Rust crate version; do not add a separate Python manifest version.

Release notes are collected under `changenote`, with the changelog at root `CHANGELOG.md`. The current single release package uses `v<version>` tags. Changenote does not yet update `Cargo.toml` from root `VERSION`. Before publication, make the Rust crate version match that release-plan version and update lockfiles; use a version that is not already published on PyPI.

This package does not provide Python bindings to the Rust library. Registry publication and a production multi-platform wheel release pipeline are not configured.
