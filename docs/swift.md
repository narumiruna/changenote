# Release Swift packages and Apple apps

Use changenote for release versions, change notes, changelogs, and Git tags.
Use the app's build and distribution tools for Apple build numbers, signing, archives, and uploads.
No Swift or Xcode executable is required to read or update an xcconfig version file.

## Select an app version source

Apple apps have two separate version values:

| Xcode build setting | Bundle key | Purpose |
| --- | --- | --- |
| `MARKETING_VERSION` | `CFBundleShortVersionString` | User-visible release version, such as `1.2.3` |
| `CURRENT_PROJECT_VERSION` | `CFBundleVersion` | Build identifier, such as `42` |

Create `Configuration/Version.xcconfig` with the app's current three-component version:

```xcconfig
// App release version
MARKETING_VERSION = 1.2.3
```

Configure a root package in `.changenote/config.json`:

```json
{
  "packages": {
    "app": {
      "path": ".",
      "version_file": "Configuration/Version.xcconfig",
      "tag_prefix": "v"
    }
  }
}
```

This configuration keeps `CHANGELOG.md` in the repository root.
The literal tag prefix keeps `v1.2.4` tags if more packages are added later.
For an app and extensions that share one release version, use one package and a shared version file.

To adopt an existing file without an existing changenote configuration, you can instead use `changenote init --name app --version-file Configuration/Version.xcconfig`.
`init` preserves the source and uses `Configuration` as the package directory, so the package changelog is also in that directory.
Use the root package configuration above if that location is not suitable.

### Supported xcconfig syntax

The `.xcconfig` extension selects this format for both `version_file` and `version_outputs`.
Use exactly one `MARKETING_VERSION` assignment with an unquoted stable SemVer value.
Spaces, tabs, LF, CRLF, and `//` comments are supported.
The file must be UTF-8 without a byte order mark (BOM).
A final newline is optional.
Only the version token is replaced; all other bytes remain unchanged.
Other settings, including `CURRENT_PROJECT_VERSION`, are not evaluated or changed.

These forms are rejected before release writes or `init` configuration creation:

- Missing or repeated `MARKETING_VERSION` assignments, even when repeated values match.
- Conditional marketing versions, such as `MARKETING_VERSION[sdk=iphoneos*]`.
- References such as `$(APP_VERSION)` or `$(inherited)`, quoted versions, and assignment operators other than `=`.
- Versions with fewer than three components, prerelease suffixes, or build metadata.
- Includes, other `#` directives, and line continuations anywhere in the configured version file.
- Block-comment delimiters outside `//` comments, and CR characters that are not part of CRLF line endings.
- A BOM, other whitespace characters, or other control characters that could hide a setting or directive.

Keep this version file separate from configuration files that need includes or other unsupported syntax.
A different, unconfigured xcconfig file can include it.
If an existing plain-text SemVer source has an `.xcconfig` extension, rename it to `VERSION` and update its configured path.
Alternatively, replace its content with the literal assignment shown above before upgrading.

## Connect the app to the version file

Create `Configuration/App.xcconfig`:

```xcconfig
#include "Version.xcconfig"
CURRENT_PROJECT_VERSION = 42
```

Do not configure `App.xcconfig` as a changenote version file: it contains an include.
In Xcode, assign it as the base configuration for every applicable project configuration, including Debug and Release.
Make sure the app and its extensions inherit the same release version.
Remove project or target `MARKETING_VERSION` overrides that would replace this value.
Inspect the resolved settings for every configuration used to build or distribute the app.

For a manually maintained `Info.plist`, use these entries inside its dictionary:

```xml
<key>CFBundleShortVersionString</key>
<string>$(MARKETING_VERSION)</string>
<key>CFBundleVersion</key>
<string>$(CURRENT_PROJECT_VERSION)</string>
```

Xcode can derive these keys from the build settings when `GENERATE_INFOPLIST_FILE` is enabled.
changenote does not edit plist or pbxproj files and cannot prove that the app uses the configured version.

### XcodeGen and other generators

Add this mapping at project level in an existing XcodeGen spec:

```yaml
configFiles:
  Debug: Configuration/App.xcconfig
  Release: Configuration/App.xcconfig
```

Match the keys to the project's configuration names.
Remove marketing-version literals from the spec so generated build settings do not override the imported value.
Regenerate the project once and commit the version file, configuration mapping, and generated project before preparing a release.
For other generators, make the same change in the generator's source, not only in its output.

If generated project files remain tracked, later marketing-version bumps must not require changes to those files.
A custom generator step that changes them in a release commit is not automatically supported by publication replay.
The existing Actions stage configured version files and compare the complete generated release tree.

### Keep an existing VERSION source

If the repository already uses a plain-text `VERSION`, retain it as the primary source and add the xcconfig as an output:

```json
{
  "packages": {
    "app": {
      "path": ".",
      "version_file": "VERSION",
      "version_outputs": ["Configuration/Version.xcconfig"],
      "tag_prefix": "v"
    }
  }
}
```

Both files must exist and contain the same stable version before planning or writing.
A release updates both, but produces one changelog, saved batch entry, and tag.
Do not run a second bump script after changenote: use one release-version calculation.

## Build numbers and distribution

A new TestFlight build can keep the same marketing version and use a new build number.
Do not add a change note only to allocate another build number.
For macOS App Store submissions, build numbers must increase across app versions.
Other Apple platforms can restart the build number for a new app version; each uploaded version/build combination must be unique.
A shared counter that continues across versions can simplify a cross-platform pipeline.

Keep build-number allocation outside changenote.
Define its scope, concurrency rules, and retry behavior in the consumer project.
Keep the app and extension versions consistent.
If Xcode's Manage version and build number option is disabled, the upload process must supply a valid new build number itself.
Inspect the final archive and exported bundle metadata before upload, even when Xcode manages these values.

The current changenote Actions require Linux.
Run Xcode validation and app distribution in separate macOS jobs.
Use the same verified release commit in every job; see the [external CI guide](github-actions.md#external-ci-and-artifacts-logice-example).
GitHub tag or Release creation does not establish App Store or TestFlight delivery.

## Swift packages

For a Swift Package Manager library, use a plain-text `VERSION` source and a full SemVer tag:

```json
{
  "packages": {
    "library": {
      "path": ".",
      "version_file": "VERSION",
      "tag_prefix": ""
    }
  }
}
```

For example, `VERSION` containing `1.2.3` and one patch note produce tag `1.2.4`.
`Package.swift` does not store the package's own release version.
Its `swift-tools-version` directive is a toolchain requirement, not a release version.
Do not configure `Package.swift` or `Package.resolved` as version files.
Swift Package Manager source releases require the package manifest at the repository root; the configuration above is not a nested-package tag convention.

## Recovery and verification limits

Upgrade CLI and Action pins together before adopting xcconfig files.
Older CLIs cannot read these version assignments.
Use matching older tooling to replay releases that used the old plain-text `.xcconfig` format.
Complete unpublished releases in that format before upgrading; do not convert their committed release batches.
Before the first release, verify the resolved Xcode settings and confirm that a generated project stays unchanged when only the version file changes.
If adoption is stopped before release, restore the previous project configuration and version source with Git.
Do not edit committed release batches or move published tags.
For a failed release write, inspect the configured outputs and follow [release-write recovery](releases.md#release-write-recovery) before retrying.

The tests cover isolated version files, configuration examples, and local Action staging and publication replay.
They do not verify consumer app integrations, signed archives, signing permissions, or App Store delivery.
Those checks remain the consumer project's responsibility before distribution.

## References

- [Apple: Preparing your app for distribution](https://developer.apple.com/documentation/xcode/preparing-your-app-for-distribution)
- [Apple: Adding a build configuration file](https://developer.apple.com/documentation/xcode/adding-a-build-configuration-file-to-your-project)
- [Apple: Build number requirements](https://developer.apple.com/documentation/xcode/setting-the-next-build-number-for-xcode-cloud-builds)
- [XcodeGen: Project specification](https://github.com/yonaskolb/XcodeGen/blob/master/Docs/ProjectSpec.md)
- [Swift Package Manager: Releasing a package](https://docs.swift.org/latest/documentation/packagemanagerdocs/releasingpublishingapackage/)
