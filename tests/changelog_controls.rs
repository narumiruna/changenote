use changenote::{
    Bump, add, add_with_type, release_notes, status, status_with_note_sections, version,
};
use semver::Version;
use serde_json::{Value, json};
use std::{fs, path::Path};

fn configure(root: &Path, changelog: Value) {
    let path = root.join(".changenote/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["changelog"] = changelog;
    fs::write(path, config.to_string()).unwrap();
}
fn project(changelog: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    changenote::init(dir.path(), "pkg", Version::new(1, 0, 0)).unwrap();
    configure(dir.path(), changelog);
    dir
}

#[test]
fn native_templates_preserve_notes_sections_headings_and_own_bumps() {
    let dir = project(json!({
        "note_template":"{package}@{version} [{bump}]\n\n{summary}",
        "sections":[{"title":"Fixes","types":["fix"]},{"title":"Features","types":["feat"]}]
    }));
    let root = dir.path();
    let note =
        "```rs\n{package} $(touch forbidden)\n```\n\n## 9.9.9 nested heading\n\n- nested item";
    add_with_type(
        root,
        "a-fix",
        vec![("pkg".into(), Bump::Patch)],
        note,
        Some("fix"),
    )
    .unwrap();
    add_with_type(
        root,
        "b-feat",
        vec![("pkg".into(), Bump::Minor)],
        "Feature",
        Some("feat"),
    )
    .unwrap();
    let plan = status(root).unwrap();
    assert_eq!(plan[0].notes, [note.to_owned(), "Feature".to_owned()]);
    let sections = status_with_note_sections(root).unwrap();
    assert_eq!(sections[0].note_sections[0].title, "Fixes");
    assert_eq!(
        sections[0].note_sections[0].notes[0],
        format!("pkg@1.1.0 [patch]\n\n{note}")
    );
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\nIntroduction\n\n## 1.0.0\n\nOld body\n",
    )
    .unwrap();
    version(root).unwrap();
    let body = release_notes(root, "pkg", &Version::new(1, 1, 0)).unwrap();
    assert!(body.starts_with("### Fixes\n\n- pkg@1.1.0 [patch]"));
    assert!(body.contains("  {package} $(touch forbidden)"));
    assert!(body.contains("  ## 9.9.9 nested heading"));
    assert!(body.contains("pkg@1.1.0 [minor]"));
    assert!(!body.contains("Old body"));
    assert!(!root.join("forbidden").exists());
    assert!(
        fs::read_to_string(root.join("CHANGELOG.md"))
            .unwrap()
            .starts_with("# Changelog\n\nIntroduction\n\n")
    );
    assert_eq!(
        release_notes(root, "pkg", &Version::new(1, 0, 0)).unwrap(),
        "Old body\n"
    );
}

#[test]
fn explicit_defaults_and_empty_object_keep_original_bytes() {
    let mut bodies = Vec::new();
    for controls in [
        json!({}),
        json!({"enabled":true}),
        json!({"note_template":"{summary}"}),
    ] {
        let dir = project(controls);
        add(
            dir.path(),
            "note",
            vec!["pkg".into()],
            Bump::Patch,
            "Fix\n\nDetails",
        )
        .unwrap();
        version(dir.path()).unwrap();
        bodies.push(fs::read(dir.path().join("CHANGELOG.md")).unwrap());
    }
    assert!(bodies.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn disabled_output_never_uses_stale_changelog_text() {
    for existing in [false, true] {
        let dir = project(json!({"enabled":false,"note_template":"{summary}"}));
        let root = dir.path();
        if existing {
            fs::write(
                root.join("CHANGELOG.md"),
                "## 1.0.1\n\nStale release body\n",
            )
            .unwrap();
        }
        let before = fs::read(root.join("CHANGELOG.md")).ok();
        add(
            root,
            "fix",
            vec!["pkg".into()],
            Bump::Patch,
            "New release body",
        )
        .unwrap();
        assert_eq!(
            status_with_note_sections(root).unwrap()[0].note_sections[0].notes,
            ["New release body"]
        );
        let report = changenote::doctor(root);
        assert!(
            !report
                .checks
                .iter()
                .any(|c| c.status == changenote::DoctorStatus::Fail)
        );
        version(root).unwrap();
        assert_eq!(fs::read(root.join("CHANGELOG.md")).ok(), before);
        assert!(!root.join(".changenote/fix.md").exists());
        assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1.0.1\n");
        assert!(
            release_notes(root, "pkg", &Version::new(1, 0, 1))
                .unwrap_err()
                .to_string()
                .contains("disabled")
        );
        let saved: Value =
            serde_json::from_slice(&fs::read(root.join(".changenote/releases.json")).unwrap())
                .unwrap();
        assert_eq!(saved[0].as_object().unwrap().len(), 5);
    }
}

#[cfg(unix)]
#[test]
fn disabled_unused_symlinks_are_not_followed_or_modified() {
    let dir = project(json!({"enabled":false}));
    let external = tempfile::tempdir().unwrap();
    let target = external.path().join("external");
    fs::write(&target, "Unrelated data").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("CHANGELOG.md")).unwrap();
    add(dir.path(), "fix", vec!["pkg".into()], Bump::Patch, "Fix").unwrap();
    version(dir.path()).unwrap();
    assert_eq!(fs::read_to_string(target).unwrap(), "Unrelated data");
    assert!(dir.path().join("CHANGELOG.md").is_symlink());
}

#[test]
fn disabled_output_still_supports_local_tags_in_a_fixture_repository() {
    use std::process::Command;
    let dir = project(json!({"enabled":false}));
    let root = dir.path();
    for args in [vec!["init", "-qb", "main"]] {
        assert!(
            Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    add(root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").unwrap();
    version(root).unwrap();
    assert!(
        Command::new("git")
            .current_dir(root)
            .args(["add", "--", "."])
            .output()
            .unwrap()
            .status
            .success()
    );
    let commit = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "fixture",
        ])
        .output()
        .unwrap();
    assert!(commit.status.success());
    assert_eq!(changenote::tag(root).unwrap(), ["v1.0.1"]);
    assert!(!root.join("CHANGELOG.md").exists());
}

#[test]
fn invalid_controls_fail_before_note_allocation_and_release_writes() {
    for controls in [
        json!({"enabled":null}),
        json!({"enabled":"false"}),
        json!({"note_template":null}),
        json!({"note_template":""}),
        json!({"note_template":"{summary} tail"}),
        json!({"note_template":"{unknown}\n\n{summary}"}),
        json!({"note_template":"```\n\n{summary}"}),
        json!({"enabled":false,"note_template":"invalid"}),
        json!({"sections":null}),
        json!({"sections":[]}),
        json!({"format":"js"}),
        json!({"note_template":true}),
    ] {
        let dir = project(controls);
        let root = dir.path();
        assert!(add(root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").is_err());
        assert!(version(root).is_err());
        assert!(!root.join(".changenote/fix.md").exists());
        assert!(!root.join("CHANGELOG.md").exists());
        assert!(!root.join(".changenote/releases.json").exists());
        assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1.0.0\n");
    }
}

#[test]
fn propagated_member_without_notes_keeps_native_explanation() {
    let dir = project(json!({"note_template":"{package} [{bump}]\n\n{summary}"}));
    let root = dir.path();
    fs::create_dir(root.join("b")).unwrap();
    fs::write(root.join("b/VERSION"), "1.0.0\n").unwrap();
    let path = root.join(".changenote/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["packages"]["b"] = json!({"path":"b","version_file":"VERSION"});
    config["fixed"] = json!([["pkg", "b"]]);
    fs::write(path, config.to_string()).unwrap();
    add(root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").unwrap();
    version(root).unwrap();
    let body = release_notes(root, "b", &Version::new(1, 0, 1)).unwrap();
    assert_eq!(
        body,
        "Released with its fixed version group; no direct changes.\n"
    );
}
