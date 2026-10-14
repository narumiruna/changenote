use changenote::{Bump, add_with_type, parse_note, status, version};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn project(sections: Option<Value>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    changenote::init(dir.path(), "type", "1.0.0".parse().unwrap()).unwrap();
    configure(dir.path(), sections);
    dir
}

fn configure(root: &Path, sections: Option<Value>) {
    let mut config = json!({"packages": {"type": {"path": ".", "version_file": "VERSION"}}});
    if let Some(sections) = sections {
        config["changelog"] = json!({"sections": sections});
    }
    fs::write(root.join(".changenote/config.json"), config.to_string()).unwrap();
}

fn sections() -> Value {
    json!([
        {"title": "Features", "types": ["feat"]},
        {"title": "Bug Fixes", "types": ["fix"]},
        {"title": "Maintenance", "types": ["chore", "docs", "style", "refactor", "test"]},
        {"title": "Performance", "types": ["perf"]}
    ])
}

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn add(root: &Path, name: &str, kind: Option<&str>, bump: Bump, message: &str) {
    add_with_type(root, name, vec![("type".into(), bump)], message, kind).unwrap();
}

#[test]
fn configured_sections_order_merge_types_and_keep_bumps_independent() {
    let dir = project(Some(sections()));
    let root = dir.path();
    add(
        root,
        "a",
        Some("fix"),
        Bump::Minor,
        "Fix\n\n```md\n## 9.0.0\n```\n\n[PR](https://example.com/1)",
    );
    add(root, "b", Some("feat"), Bump::Patch, "Feature");
    add(root, "c", Some("docs"), Bump::Patch, "Docs");
    add(root, "d", Some("refactor"), Bump::Major, "Refactor");
    add(root, "e", None, Bump::Patch, "Untyped");
    let default = cli(root, &["status"]);
    let default: Value = serde_json::from_slice(&default.stdout).unwrap();
    assert!(default[0].get("note_types").is_none());
    assert!(default[0].get("note_sections").is_none());
    let detailed = cli(
        root,
        &[
            "status",
            "--include-note-types",
            "--include-note-sections",
            "--include-note-bumps",
            "--include-note-paths",
        ],
    );
    let detail: Value = serde_json::from_slice(&detailed.stdout).unwrap();
    assert_eq!(
        detail[0]["note_types"],
        json!(["fix", "feat", "docs", "refactor", null])
    );
    assert_eq!(
        detail[0]["note_bumps"],
        json!(["minor", "patch", "patch", "major", "patch"])
    );
    assert_eq!(detail[0]["note_paths"][0], ".changenote/a.md");
    assert_eq!(
        detail[0]["note_sections"][2],
        json!({"title":"Maintenance", "notes":["Docs", "Refactor"]})
    );
    assert_eq!(detail[0]["note_sections"].as_array().unwrap().len(), 4);
    assert_eq!(status(root).unwrap()[0].version.to_string(), "2.0.0");
    assert!(!root.join("CHANGELOG.md").exists());
    assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1.0.0\n");
    version(root).unwrap();
    let log = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert_eq!(
        log,
        "## 2.0.0\n\n### Features\n\n- Feature\n\n### Bug Fixes\n\n- Fix\n  \n  ```md\n  ## 9.0.0\n  ```\n  \n  [PR](https://example.com/1)\n\n### Maintenance\n\n- Docs\n\n- Refactor\n\n### Other Changes\n\n- Untyped\n\n"
    );
    assert_eq!(
        changenote::release_notes(root, "type", &"2.0.0".parse().unwrap()).unwrap(),
        log.strip_prefix("## 2.0.0\n\n")
            .unwrap()
            .trim_end()
            .to_owned()
            + "\n"
    );
    let saved: Value =
        serde_json::from_slice(&fs::read(root.join(".changenote/releases.json")).unwrap()).unwrap();
    assert_eq!(saved[0].as_object().unwrap().len(), 5);
    assert!(status(root).unwrap().is_empty());
}

#[test]
fn typed_notes_preserve_bump_grouping_without_section_config() {
    let dir = project(None);
    let root = dir.path();
    let result = cli(
        root,
        &[
            "add", "named", "--patch", "type", "--type", "custom", "-m", "Typed",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let types = cli(root, &["status", "--include-note-types"]);
    let types: Value = serde_json::from_slice(&types.stdout).unwrap();
    assert_eq!(types[0]["note_types"], json!(["custom"]));
    assert!(types[0].get("note_paths").is_none());
    assert!(types[0].get("note_sections").is_none());
    let sections = cli(root, &["status", "--include-note-sections"]);
    let sections: Value = serde_json::from_slice(&sections.stdout).unwrap();
    assert_eq!(
        sections[0]["note_sections"],
        json!([{"title":"Patch Changes", "notes":["Typed"]}])
    );
    assert!(sections[0].get("note_types").is_none());
    let content = fs::read_to_string(root.join(".changenote/named.md")).unwrap();
    assert!(content.contains("$type: custom"));
    assert_eq!(parse_note(&content).unwrap().packages["type"], Bump::Patch);
    let result = cli(
        root,
        &[
            "add",
            "--minor",
            "type",
            "--type",
            "feat",
            "-m",
            "Generated",
        ],
    );
    assert!(result.status.success());
    let path = String::from_utf8(result.stdout).unwrap();
    assert!(
        fs::read_to_string(root.join(path.trim()))
            .unwrap()
            .contains("$type: feat")
    );
    version(root).unwrap();
    let log = fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert_eq!(
        log,
        "## 1.1.0\n\n### Minor Changes\n\n- Generated\n\n### Patch Changes\n\n- Typed\n\n"
    );
}

#[test]
fn invalid_config_and_types_fail_before_writes() {
    let dir = project(None);
    let root = dir.path();
    for invalid in [
        json!([]),
        json!([{"title":"Features", "types":[]}]),
        json!([{"title":"", "types":["feat"]}]),
        json!([{"title":" Features", "types":["feat"]}]),
        json!([{"title":"Bad\nHeading", "types":["feat"]}]),
        json!([{"title":"Other Changes", "types":["feat"]}]),
        json!([{"title":"1.0.0", "types":["feat"]}]),
        json!([{"title":"1.0.0 Features", "types":["feat"]}]),
        json!([{"title":"A", "types":["feat", "feat"]}]),
        json!([{"title":"A", "types":["feat"]}, {"title":"B", "types":["feat"]}]),
        json!([{"title":"A", "types":["feat"]}, {"title":"A", "types":["fix"]}]),
        json!([{"title":"A", "types":["bad type"]}]),
        json!([{"title":"A", "types":["feat"], "extra":true}]),
        json!([{"title":"A"}]),
        json!(null),
    ] {
        configure(root, Some(invalid));
        assert!(status(root).is_err());
        assert!(
            add_with_type(
                root,
                "invalid",
                vec![("type".into(), Bump::Patch)],
                "Fix",
                Some("feat")
            )
            .is_err()
        );
        assert!(!root.join(".changenote/invalid.md").exists());
    }
    configure(root, Some(sections()));
    for kind in ["unknown", "", "bad type", "$type"] {
        for name in [Some("invalid"), None] {
            let mut args = vec!["add"];
            if let Some(name) = name {
                args.push(name);
            }
            args.extend(["--patch", "type", "--type", kind, "-m", "Fix"]);
            assert!(!cli(root, &args).status.success());
        }
    }
    assert_eq!(fs::read_dir(root.join(".changenote")).unwrap().count(), 1);
    fs::write(
        root.join(".changenote/bad.md"),
        "---\ntype: patch\n$type: unknown\n---\nFix",
    )
    .unwrap();
    assert!(status(root).is_err());
    assert!(version(root).is_err());
    assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1.0.0\n");
    assert!(!root.join("CHANGELOG.md").exists());
    assert!(!root.join(".changenote/releases.json").exists());
    assert!(root.join(".changenote/bad.md").exists());
}

#[test]
fn pr_check_validates_typed_notes_even_with_an_exemption() {
    let dir = project(Some(sections()));
    let root = dir.path();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .current_dir(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init", "-b", "main"]);
    git(&["add", "--", ".changenote/config.json", "VERSION"]);
    git(&["commit", "-m", "initial"]);
    git(&["branch", "base"]);
    add(root, "valid", Some("feat"), Bump::Minor, "Feature");
    git(&["add", "--", ".changenote/valid.md"]);
    git(&["commit", "-m", "typed note"]);
    assert!(changenote::check(root, "base", false, None).unwrap().passed);
    fs::write(
        root.join(".changenote/valid.md"),
        "---\ntype: minor\n$type: unknown\n---\nFeature",
    )
    .unwrap();
    git(&["add", "--", ".changenote/valid.md"]);
    git(&["commit", "-m", "invalid type"]);
    assert!(changenote::check(root, "base", false, None).is_err());
    assert!(changenote::check(root, "base", true, Some("Documentation only")).is_err());
}

#[test]
fn malformed_metadata_is_rejected_and_type_package_remains_valid() {
    for metadata in ["null", "[]", "{}", "42", "''"] {
        assert!(parse_note(&format!("---\ntype: patch\n$type: {metadata}\n---\nFix")).is_err());
    }
    assert!(parse_note("---\ntype: patch\n$type: feat\n$type: fix\n---\nFix").is_err());
    assert!(parse_note("---\n$type: feat\n---\nFix").is_err());
    assert_eq!(
        parse_note("---\r\ntype: patch\r\n$type: fix\r\n---\r\nFix")
            .unwrap()
            .packages["type"],
        Bump::Patch
    );
}

#[test]
fn custom_sections_work_with_mixed_bumps_and_fixed_groups() {
    let dir = project(Some(sections()));
    let root = dir.path();
    fs::create_dir(root.join("b")).unwrap();
    fs::write(root.join("b/VERSION"), "1.0.0\n").unwrap();
    let mut config: Value =
        serde_json::from_slice(&fs::read(root.join(".changenote/config.json")).unwrap()).unwrap();
    config["packages"]["b"] = json!({"path":"b", "version_file":"VERSION"});
    fs::write(root.join(".changenote/config.json"), config.to_string()).unwrap();
    assert!(
        cli(
            root,
            &[
                "add", "mixed", "--minor", "type", "--patch", "b", "--type", "feat", "-m", "Shared"
            ]
        )
        .status
        .success()
    );
    let releases = status(root).unwrap();
    assert_eq!(releases[0].version.to_string(), "1.0.1");
    assert_eq!(releases[1].version.to_string(), "1.1.0");
    version(root).unwrap();
    for path in ["CHANGELOG.md", "b/CHANGELOG.md"] {
        assert!(
            fs::read_to_string(root.join(path))
                .unwrap()
                .contains("### Features\n\n- Shared")
        );
    }
    config["fixed"] = json!([["type", "b"]]);
    fs::write(root.join(".changenote/config.json"), config.to_string()).unwrap();
    add(root, "fix", Some("fix"), Bump::Patch, "Fix");
    let details = cli(
        root,
        &[
            "status",
            "--include-note-sections",
            "--include-note-types",
            "--include-note-paths",
        ],
    );
    let details: Value = serde_json::from_slice(&details.stdout).unwrap();
    assert_eq!(details[0]["note_sections"], json!([]));
    assert_eq!(details[0]["note_types"], json!([]));
    version(root).unwrap();
    assert!(
        fs::read_to_string(root.join("b/CHANGELOG.md"))
            .unwrap()
            .contains("## 1.1.1\n\nReleased with its fixed version group; no direct changes.")
    );
}
