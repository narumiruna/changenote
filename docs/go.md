# Release Go modules

Use a plain-text `VERSION` file as the release version source. Go modules already work with changenote; no Go executable or new configuration field is required.

`go.mod` does not contain the module's release version. Its `go` directive selects a Go language version. Do not configure `go.mod` as a version file. changenote does not read module directives, infer module paths, or check that a tag matches a Go module path.

## Select the tag prefix

The following matrix uses a repository at `example.com/repo`. Each row is a separate layout, not one combined configuration. `VERSION` contains a stable SemVer string without a leading `v`. These examples show the result of a patch note.

| Module path | Repository directory (`path`) | Current `VERSION` | `tag_prefix` | Next tag |
| --- | --- | --- | --- | --- |
| `example.com/repo` | `.` | `0.2.0` | `v` | `v0.2.1` |
| `example.com/repo` | `.` | `1.2.0` | `v` | `v1.2.1` |
| `example.com/repo/v2` | `.` | `2.2.0` | `v` | `v2.2.1` |
| `example.com/repo/v2` | `v2` | `2.2.0` | `v` | `v2.2.1` |
| `example.com/repo/sdk` | `sdk` | `0.2.0` | `sdk/v` | `sdk/v0.2.1` |
| `example.com/repo/sdk` | `sdk` | `1.2.0` | `sdk/v` | `sdk/v1.2.1` |
| `example.com/repo/sdk/v2` | `sdk` | `2.2.0` | `sdk/v` | `sdk/v2.2.1` |
| `example.com/repo/sdk/v2` | `sdk/v2` | `2.2.0` | `sdk/v` | `sdk/v2.2.1` |

Each `VERSION` file is in the configured package directory beside `go.mod`. A physical major-version directory does not add another component to the tag prefix: use `sdk/v2.2.1`, not `sdk/v2/v2.2.1`.

Set `tag_prefix` explicitly, including `v` for a root module. Otherwise, adding a second package changes the default tags to `changenote/<package>@v<version>`, which is not the Go module tag convention.

Sources, checked on 2026-10-24:

- [Go Modules Reference: Module paths](https://go.dev/ref/mod#module-path) defines the repository path, module subdirectory, and major version suffix.
- [Mapping versions to commits](https://go.dev/ref/mod#vcs-version) defines root and subdirectory tag names and immutable tags.
- [Module directories within a repository](https://go.dev/ref/mod#vcs-dir) permits v2+ sources with or without a physical major-version directory.
- [Major version suffixes](https://go.dev/ref/mod#major-version-suffixes) requires `/v2` and later suffixes for ordinary module paths. The special `gopkg.in` convention and legacy `+incompatible` releases are outside these examples.

## Configure root and nested modules

For a root module at `example.com/repo` and a nested module at `example.com/repo/sdk/v2`, use this layout:

```text
.changenote/config.json
go.mod                 # module example.com/repo
VERSION                # 1.2.0
sdk/v2/go.mod          # module example.com/repo/sdk/v2
sdk/v2/VERSION         # 2.2.0
```

Create `.changenote/config.json`:

```json
{
  "$schema": "https://raw.githubusercontent.com/narumiruna/changenote/main/schema.json",
  "packages": {
    "pkg": {
      "path": ".",
      "version_file": "VERSION",
      "tag_prefix": "v"
    },
    "sdk": {
      "path": "sdk/v2",
      "version_file": "VERSION",
      "tag_prefix": "sdk/v"
    }
  }
}
```

For a root module only, omit `sdk`. For a nested module only, omit `pkg`. Keep the explicit tag prefixes in either case.

To adopt one existing `VERSION` file in a repository without a configuration, run `changenote init --name pkg --version-file VERSION`, or `changenote init --name sdk --version-file sdk/v2/VERSION`. Then set the matching `tag_prefix` in the generated configuration. `init` preserves the existing version file; it does not discover other modules.

## Prepare and review a release

Run these commands from the consumer repository root with an installed compatible CLI. They are release examples, not instructions to release changenote itself.

```sh
changenote add fix-api --patch pkg --patch sdk --message "Fix API error handling."
changenote status
```

The plan must show `pkg` at `1.2.1` with tag `v1.2.1`, and `sdk` at `2.2.1` with tag `sdk/v2.2.1`. For a single module, name only that package in `add`.

Check module paths, imports, and tag prefixes before applying the plan. Run the Go tests appropriate for each module separately from changenote. For a v2+ migration, update the module directive and affected imports yourself, and review downstream callers. A major bump alone does not perform this migration or validate Go compatibility.

Commit the change note with its implementation before release approval. The commands below assume that `.changenote/fix-api.md` is tracked and the working tree is clean.

After release approval:

```sh
changenote version
git diff -- .changenote VERSION CHANGELOG.md sdk/v2/VERSION sdk/v2/CHANGELOG.md
git status --short
```

Review both version files, both changelogs, the saved batch in `.changenote/releases.json`, and the deleted note. `go.mod`, `go.sum`, `go.work`, source files, and dependency requirements must remain unchanged by changenote. It does not run `go mod tidy`, install dependencies, or download modules.

Stage only release outputs and the consumed note, then commit:

```sh
git add -- .changenote/releases.json .changenote/fix-api.md VERSION CHANGELOG.md sdk/v2/VERSION sdk/v2/CHANGELOG.md
git commit -m "chore(release): version Go modules"
changenote tag
```

With a clean working tree, `tag` creates local annotated tags `v1.2.1` and `sdk/v2.2.1`. It does not push. Push only after separate publication approval and repository review. Never move an existing published tag; correct release errors with a new version. Do not edit committed saved batches.

## Actions and limits

The existing Release PR, preview, and publication Actions use these configured version files and literal tag prefixes. Use compatible CLI and Action pins, and follow the permissions and clean-checkout rules in [README.md](../README.md#github-actions). No Go-specific Action setting is required.

Release PR staging includes configured version files, package changelogs, saved batch data, and consumed notes. Publication reconstructs the complete release tree. It does not publish to a Go proxy or update internal dependency requirements. Local fixtures verify staging and reconstruction with local Git repositories and a GitHub mock; they do not verify hosted permissions, remote module fetching, or proxy indexing.
