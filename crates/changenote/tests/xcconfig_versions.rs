use changenote::{init_with_version_file, load_config};
use semver::Version;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn source(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn configure(root: &Path, primary: &str, outputs: &[&str]) {
    source(
        root,
        ".changenote/config.json",
        &json!({"packages": {"app": {
            "path": ".", "version_file": primary, "version_outputs": outputs
        }}})
        .to_string(),
    );
    source(
        root,
        ".changenote/change.md",
        "---\napp: minor\n---\nAdd an app feature.\n",
    );
}

fn fixture(content: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    source(dir.path(), "Configuration/Version.xcconfig", content);
    configure(dir.path(), "Configuration/Version.xcconfig", &[]);
    dir
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap()
}

#[test]
fn updates_only_the_marketing_version_token() {
    for before in [
        "MARKETING_VERSION=1.2.3",
        "// /* MARKETING_VERSION = 9.9.9 */\nMARKETING_VERSION = 1.2.3\n",
        "// Version 設定\r\n// MARKETING_VERSION = 9.9.9\r\n\tMARKETING_VERSION \t=\t1.2.3 \t// release 1.2.3\r\nCURRENT_PROJECT_VERSION = 1.2.3\r\n",
        "PRODUCT_NAME = Example\nMARKETING_VERSION = 1.2.3 // release\nCURRENT_PROJECT_VERSION = 42\nOTHER_SETTING[sdk=macosx*] = 1.2.3\n",
        "MY_MARKETING_VERSION = 9.9.9\nMARKETING_VERSION_SUFFIX = 9.9.9\nMARKETING_VERSION = 1.2.3\n",
        "\n\t// MARKETING_VERSION[sdk=iphoneos*] = 9.9.9\n MARKETING_VERSION = 1.2.3 \t",
    ] {
        let dir = fixture(before);
        let root = dir.path();
        let plan = changenote::status(root).unwrap();
        assert_eq!(plan[0].old_version, Version::new(1, 2, 3));
        assert_eq!(plan[0].version, Version::new(1, 3, 0));
        assert_eq!(read(root, "Configuration/Version.xcconfig"), before);
        let release = changenote::version(root).unwrap();
        assert_eq!(release[0].tag, "v1.3.0");
        let start = before.find("1.2.3").unwrap();
        let mut expected = before.to_owned();
        expected.replace_range(start..start + "1.2.3".len(), "1.3.0");
        assert_eq!(read(root, "Configuration/Version.xcconfig"), expected);
        assert!(!root.join(".changenote/change.md").exists());
        let batch = read(root, ".changenote/releases.json");
        let saved: Value = serde_json::from_str(&batch).unwrap();
        assert_eq!(saved[0].as_object().unwrap().len(), 5);
        assert!(changenote::version(root).unwrap().is_empty());
        assert_eq!(read(root, ".changenote/releases.json"), batch);
    }
}

#[test]
fn init_adopts_nested_xcconfig_without_changing_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let path = "Configuration/App.release.xcconfig";
    let before = "// release\r\nMARKETING_VERSION = 1.2.3\r\nCURRENT_PROJECT_VERSION = 42\r\n";
    source(root, path, before);
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(["init", "--name", "app", "--version-file", path])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let config = load_config(root).unwrap();
    assert_eq!(config.packages["app"].path, Path::new("Configuration"));
    assert_eq!(
        config.packages["app"].version_file,
        Path::new("App.release.xcconfig")
    );
    assert_eq!(read(root, path), before);
    assert!(!root.join("VERSION").exists());
    assert!(changenote::status(root).unwrap().is_empty());
}

#[test]
fn invalid_xcconfig_fails_before_init_or_release_writes() {
    for before in [
        "",
        "1.2.3\n",
        "CURRENT_PROJECT_VERSION = 42\n",
        "// MARKETING_VERSION = 1.2.3\n",
        "MARKETING_VERSION =\n",
        "MARKETING_VERSION = 1.2\n",
        "MARKETING_VERSION = 01.2.3\n",
        "MARKETING_VERSION = 1.2.3-beta.1\n",
        "MARKETING_VERSION = 1.2.3+42\n",
        "MARKETING_VERSION = $(APP_VERSION)\n",
        "MARKETING_VERSION = ${APP_VERSION}\n",
        "MARKETING_VERSION = $(inherited) 1.2.3\n",
        "MARKETING_VERSION = \"1.2.3\"\n",
        "MARKETING_VERSION = '1.2.3'\n",
        "MARKETING_VERSION = 1.2.3;\n",
        "MARKETING_VERSION = 1.2.3 4.5.6\n",
        "MARKETING_VERSION = 1.2.3\nMARKETING_VERSION = 1.2.3\n",
        "MARKETING_VERSION = 1.2.3\nMARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION[sdk=iphoneos*] = 1.2.3\n",
        "MARKETING_VERSION = 1.2.3\nMARKETING_VERSION[config=Release] = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nMARKETING_VERSION [sdk=macosx*] = 9.9.9\n",
        "MARKETING_VERSION += 1.2.3\n",
        "MARKETING_VERSION ?= 1.2.3\n",
        "MARKETING_VERSION := 1.2.3\n",
        "MARKETING_VERSION == 1.2.3\n",
        "#include \"Other.xcconfig\"\nMARKETING_VERSION = 1.2.3\n",
        "MARKETING_VERSION = 1.2.3\n#include? \"Other.xcconfig\"\n",
        "#if DEBUG\nMARKETING_VERSION = 1.2.3\n#endif\n",
        "MARKETING_VERSION = \\\n1.2.3\n",
        "MARKETING_VER\\\nSION = 9.9.9\nMARKETING_VERSION = 1.2.3\n",
        "OTHER_SETTING = value \\\nMARKETING_VERSION = 1.2.3\n",
        "OTHER_SETTING = value \\ // comment\nMARKETING_VERSION = 1.2.3\n",
        "OTHER_SETTING = value \\// comment\nMARKETING_VERSION = 1.2.3\n",
        "OTHER_SETTING = value \\\t// comment\nMARKETING_VERSION = 1.2.3\n",
        "// continued comment \\\nMARKETING_VERSION = 1.2.3\n",
        "/*\nMARKETING_VERSION = 9.9.9\n*/\nMARKETING_VERSION = 1.2.3\n",
        "MARKETING_VERSION /* version */ = 1.2.3\n",
        "MARKETING_VERSION = 1.2.3 /* release */\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\rMARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\n// comment\rMARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\r",
        "MARKETING_VERSION = 1.2.3\r\r\n",
        "\u{feff}#include \"Extra.xcconfig\"\nMARKETING_VERSION = 1.2.3\n",
        "\u{feff}MARKETING_VERSION[config=Release] = 9.9.9\nMARKETING_VERSION = 1.2.3\n",
        "\u{feff}MARKETING_VERSION = 9.9.9\nMARKETING_VERSION = 1.2.3\n",
        "MARKETING_VERSION = 1.2.3\n\u{feff}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\n\u{a0}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\u{85}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\u{2028}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\n// comment\u{2029}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\u{b}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\u{c}MARKETING_VERSION = 9.9.9\n",
        "MARKETING_VERSION = 1.2.3\nOTHER = value\0MARKETING_VERSION = 9.9.9\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let path = "Version.xcconfig";
        source(root, path, before);
        assert!(
            init_with_version_file(root, "app", Path::new(path)).is_err(),
            "init accepted {before:?}"
        );
        assert!(!root.join(".changenote").exists());
        configure(root, path, &[]);
        source(root, "CHANGELOG.md", "# Existing changelog\n");
        source(root, ".changenote/releases.json", "[]\n");
        assert!(
            changenote::status(root).is_err(),
            "status accepted {before:?}"
        );
        assert!(
            changenote::version(root).is_err(),
            "version accepted {before:?}"
        );
        assert_eq!(read(root, path), before);
        assert_eq!(read(root, "CHANGELOG.md"), "# Existing changelog\n");
        assert_eq!(read(root, ".changenote/releases.json"), "[]\n");
        assert_eq!(
            read(root, ".changenote/change.md"),
            "---\napp: minor\n---\nAdd an app feature.\n"
        );
    }
}

#[test]
fn primary_and_additional_xcconfigs_update_without_external_tools() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let app = "MARKETING_VERSION = 1.2.3\nCURRENT_PROJECT_VERSION = 42\n";
    let widget = "MARKETING_VERSION = 1.2.3 // widget\r\nCURRENT_PROJECT_VERSION = 42\r\n";
    source(root, "Configuration/Version.xcconfig", app);
    source(root, "Widget.xcconfig", widget);
    source(root, "VERSION", "1.2.3\n");
    configure(
        root,
        "Configuration/Version.xcconfig",
        &["Widget.xcconfig", "VERSION"],
    );
    for path in [
        "Cargo.lock",
        "uv.lock",
        "package-lock.json",
        "Package.resolved",
    ] {
        source(root, path, "unchanged\n");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .env("PATH", root.join("no-executables"))
        .arg("version")
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_eq!(
        read(root, "Configuration/Version.xcconfig"),
        app.replace("1.2.3", "1.3.0")
    );
    assert_eq!(
        read(root, "Widget.xcconfig"),
        widget.replace("1.2.3", "1.3.0")
    );
    assert_eq!(read(root, "VERSION"), "1.3.0\n");
    for path in [
        "Cargo.lock",
        "uv.lock",
        "package-lock.json",
        "Package.resolved",
    ] {
        assert_eq!(read(root, path), "unchanged\n");
    }
}

#[test]
fn additional_xcconfig_must_match_the_primary_version() {
    for before in [
        "MARKETING_VERSION = 9.9.9\nCURRENT_PROJECT_VERSION = 42\n",
        "MARKETING_VERSION = $(APP_VERSION)\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        source(root, "VERSION", "1.2.3\n");
        source(root, "Version.xcconfig", before);
        configure(root, "VERSION", &["Version.xcconfig"]);
        assert!(changenote::status(root).is_err());
        assert!(changenote::version(root).is_err());
        assert_eq!(read(root, "VERSION"), "1.2.3\n");
        assert_eq!(read(root, "Version.xcconfig"), before);
        assert!(root.join(".changenote/change.md").exists());
        assert!(!root.join("CHANGELOG.md").exists());
        assert!(!root.join(".changenote/releases.json").exists());
    }
}

#[test]
fn documented_app_and_swift_package_configurations_work() {
    let guide = include_str!("../../../docs/swift.md");
    let xcconfig = guide
        .split_once("```xcconfig\n")
        .unwrap()
        .1
        .split_once("```")
        .unwrap()
        .0;
    let mut examples = 0;
    for block in guide.split("```json\n").skip(1) {
        let config_text = block.split_once("```").unwrap().0;
        let config: Value = serde_json::from_str(config_text).unwrap();
        let packages = config["packages"].as_object().unwrap();
        assert_eq!(packages.len(), 1);
        let (name, package) = packages.iter().next().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        source(root, ".changenote/config.json", config_text);
        source(
            root,
            ".changenote/change.md",
            &format!("---\n{name}: patch\n---\nFix behavior.\n"),
        );
        let mut paths = vec![package["version_file"].as_str().unwrap()];
        if let Some(outputs) = package["version_outputs"].as_array() {
            paths.extend(outputs.iter().map(|path| path.as_str().unwrap()));
        }
        let package_dir = package["path"].as_str().unwrap();
        for path in &paths {
            source(
                &root.join(package_dir),
                path,
                if path.ends_with(".xcconfig") {
                    xcconfig
                } else {
                    "1.2.3\n"
                },
            );
        }
        let manifest = "// swift-tools-version: 6.2\n";
        source(root, "Package.swift", manifest);
        let plan = changenote::status(root).unwrap();
        assert_eq!(plan[0].old_version, Version::new(1, 2, 3));
        assert_eq!(plan[0].version, Version::new(1, 2, 4));
        assert_eq!(
            plan[0].tag,
            format!("{}1.2.4", package["tag_prefix"].as_str().unwrap())
        );
        changenote::version(root).unwrap();
        for path in paths {
            assert_eq!(
                read(&root.join(package_dir), path),
                if path.ends_with(".xcconfig") {
                    xcconfig.replace("1.2.3", "1.2.4")
                } else {
                    "1.2.4\n".to_owned()
                }
            );
        }
        assert_eq!(read(root, "Package.swift"), manifest);
        examples += 1;
    }
    assert_eq!(examples, 3);
}

#[test]
fn fixed_group_updates_xcconfigs_without_changing_build_counters() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    source(
        root,
        "ios/Version.xcconfig",
        "MARKETING_VERSION = 1.0.0\nCURRENT_PROJECT_VERSION = 42\n",
    );
    source(
        root,
        "macos/Version.xcconfig",
        "MARKETING_VERSION = 2.0.0\nCURRENT_PROJECT_VERSION = 81\n",
    );
    source(
        root,
        ".changenote/config.json",
        &json!({"packages": {
            "ios": {"path": "ios", "version_file": "Version.xcconfig"},
            "macos": {"path": "macos", "version_file": "Version.xcconfig"}
        }, "fixed": [["ios", "macos"]]})
        .to_string(),
    );
    source(
        root,
        ".changenote/change.md",
        "---\nios: patch\n---\nFix the iOS app.\n",
    );
    let releases = changenote::version(root).unwrap();
    assert_eq!(releases.len(), 2);
    assert_eq!(
        read(root, "ios/Version.xcconfig"),
        "MARKETING_VERSION = 2.0.1\nCURRENT_PROJECT_VERSION = 42\n"
    );
    assert_eq!(
        read(root, "macos/Version.xcconfig"),
        "MARKETING_VERSION = 2.0.1\nCURRENT_PROJECT_VERSION = 81\n"
    );
    assert!(releases[1].notes.is_empty());
}
