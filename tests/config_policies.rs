use changenote::{Bump, add, add_with_bumps, status, status_with_group_causes, version};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn project(settings: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    let mut packages = serde_json::Map::new();
    for name in ["a", "b", "c", "d"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join("VERSION"), "1.0.0\n").unwrap();
        packages.insert(name.into(), json!({"path":name,"version_file":"VERSION"}));
    }
    let mut config = json!({"packages":packages});
    for (key, value) in settings.as_object().unwrap() {
        config[key] = value.clone();
    }
    fs::write(
        dir.path().join(".changenote/config.json"),
        config.to_string(),
    )
    .unwrap();
    dir
}

fn setting(root: &Path, key: &str, value: Value) {
    let path = root.join(".changenote/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config[key] = value;
    fs::write(path, config.to_string()).unwrap();
}

fn package_setting(root: &Path, name: &str, key: &str, value: Value) {
    let mut config: Value =
        serde_json::from_slice(&fs::read(root.join(".changenote/config.json")).unwrap()).unwrap();
    config["packages"][name][key] = value;
    setting(root, "packages", config["packages"].clone());
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

fn note(root: &Path, name: &str, packages: &[(&str, Bump)]) {
    add_with_bumps(
        root,
        name,
        packages
            .iter()
            .map(|(name, bump)| (name.to_string(), *bump))
            .collect(),
        "Same summary",
    )
    .unwrap();
}

fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn linked_baseline_releases_only_active_members_and_preserves_public_views() {
    let dir = project(json!({"linked":[["b","a"]]}));
    let root = dir.path();
    fs::write(root.join("b/VERSION"), "2.4.9\n").unwrap();
    fs::write(root.join("a/SECOND"), "1.0.0\n").unwrap();
    fs::write(root.join("b/SECOND"), "2.4.9\n").unwrap();
    package_setting(root, "a", "version_outputs", json!(["SECOND"]));
    package_setting(root, "b", "version_outputs", json!(["SECOND"]));
    note(root, "a-note", &[("a", Bump::Patch)]);
    let before = snapshot(root);
    let releases = status_with_group_causes(root).unwrap();
    assert_eq!(snapshot(root), before);
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].detail.release.version.to_string(), "2.4.10");
    assert_eq!(releases[0].detail.release.old_version.to_string(), "1.0.0");
    let group = releases[0].linked_group.as_ref().unwrap();
    assert_eq!(group.members, ["a", "b"]);
    assert_eq!(group.base_version.to_string(), "2.4.9");
    assert_eq!(
        group.source_note_paths,
        [PathBuf::from(".changenote/a-note.md")]
    );
    let _old = changenote::SectionedRelease {
        detail: changenote::DetailedRelease {
            release: releases[0].detail.release.clone(),
            note_paths: vec![],
            fixed_group: None,
        },
        note_types: vec![],
        note_sections: vec![],
    };
    assert!(
        changenote::status_with_note_paths(root).unwrap()[0]
            .fixed_group
            .is_none()
    );
    let default: Value = serde_json::from_slice(&cli(root, &["status"]).stdout).unwrap();
    assert!(default[0].get("linked_group").is_none());
    let detailed: Value =
        serde_json::from_slice(&cli(root, &["status", "--include-note-paths"]).stdout).unwrap();
    assert_eq!(detailed[0]["linked_group"]["members"], json!(["a", "b"]));
    version(root).unwrap();
    for name in ["b", "c", "d"] {
        assert_eq!(
            fs::read(root.join(name).join("VERSION")).unwrap(),
            before[&PathBuf::from(format!("{name}/VERSION"))]
        );
        assert!(!root.join(name).join("CHANGELOG.md").exists());
    }
    assert_eq!(
        fs::read_to_string(root.join("a/SECOND")).unwrap(),
        "2.4.10\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b/SECOND")).unwrap(),
        "2.4.9\n"
    );
    let batch: Value =
        serde_json::from_slice(&fs::read(root.join(".changenote/releases.json")).unwrap()).unwrap();
    assert_eq!(batch[0].as_object().unwrap().len(), 5);
    assert!(status(root).unwrap().is_empty());
}

#[test]
fn linked_mixed_bumps_keep_direct_attribution_and_original_baseline() {
    let dir = project(json!({"linked":[["a","b"]], "fixed":[["c","d"]]}));
    let root = dir.path();
    fs::write(root.join("b/VERSION"), "1.2.0\n").unwrap();
    note(root, "first", &[("a", Bump::Patch), ("c", Bump::Patch)]);
    note(root, "second", &[("b", Bump::Minor)]);
    let releases = status_with_group_causes(root).unwrap();
    assert_eq!(releases.len(), 4);
    for release in &releases[..2] {
        assert_eq!(release.detail.release.version.to_string(), "1.3.0");
        assert_eq!(release.detail.release.notes.len(), 1);
        assert_eq!(release.detail.note_paths.len(), 1);
        assert_eq!(
            release
                .linked_group
                .as_ref()
                .unwrap()
                .source_note_paths
                .len(),
            2
        );
    }
    assert!(releases[2].linked_group.is_none());
    assert!(releases[2].detail.fixed_group.is_some());
    assert!(releases[3].detail.release.notes.is_empty());
    assert_eq!(releases[0].detail.release.note_bumps, [Bump::Patch]);
}

#[test]
fn ignore_retains_notes_and_linked_baselines_without_mutating_paused_outputs() {
    let dir = project(json!({"ignore":["b"],"linked":[["a","b"]]}));
    let root = dir.path();
    fs::write(root.join("b/VERSION"), "2.4.9\n").unwrap();
    fs::write(root.join("b/SECOND"), "2.4.9\n").unwrap();
    fs::write(root.join("b/CHANGELOG.md"), "Old content\n").unwrap();
    package_setting(root, "b", "version_outputs", json!(["SECOND"]));
    note(root, "paused", &[("b", Bump::Minor)]);
    let paused = fs::read_to_string(root.join(".changenote/paused.md"))
        .unwrap()
        .replace('\n', "\r\n");
    fs::write(root.join(".changenote/paused.md"), &paused).unwrap();
    note(root, "active", &[("a", Bump::Patch)]);
    let plan = status(root).unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].version.to_string(), "2.4.10");
    version(root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join(".changenote/paused.md")).unwrap(),
        paused
    );
    assert!(!root.join(".changenote/active.md").exists());
    assert_eq!(
        fs::read_to_string(root.join("b/VERSION")).unwrap(),
        "2.4.9\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b/SECOND")).unwrap(),
        "2.4.9\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("b/CHANGELOG.md")).unwrap(),
        "Old content\n"
    );
    let before = snapshot(root);
    assert!(version(root).unwrap().is_empty());
    assert_eq!(snapshot(root), before);
    let report = changenote::doctor(root);
    assert!(
        report
            .checks
            .iter()
            .find(|c| c.name == "plan")
            .unwrap()
            .message
            .starts_with("0 ")
    );
}

#[test]
fn ignored_fixed_groups_and_saved_batches_remain_unchanged() {
    let dir = project(json!({"ignore":["a","b"],"fixed":[["a*","b"]]}));
    note(
        dir.path(),
        "paused",
        &[("a", Bump::Patch), ("b", Bump::Minor)],
    );
    fs::write(dir.path().join(".changenote/releases.json"), "[]\n").unwrap();
    let before = snapshot(dir.path());
    assert!(version(dir.path()).unwrap().is_empty());
    assert_eq!(snapshot(dir.path()), before);
    fs::write(dir.path().join("a/VERSION"), "invalid").unwrap();
    assert!(status(dir.path()).is_err());
}

#[test]
fn ignore_rejects_invalid_lists_and_mixed_notes_before_allocation_or_writes() {
    for ignore in [
        json!(null),
        json!(["missing"]),
        json!(["a", "a"]),
        json!(["a*"]),
        json!("a"),
    ] {
        let dir = project(json!({"ignore":ignore}));
        let before = snapshot(dir.path());
        assert!(status(dir.path()).is_err());
        assert!(version(dir.path()).is_err());
        assert_eq!(snapshot(dir.path()), before);
    }
    let dir = project(json!({"ignore":["a"],"fixed":[["a","b"]]}));
    assert!(status(dir.path()).is_err());
    let dir = project(json!({"ignore":["b"]}));
    let root = dir.path();
    let before = snapshot(root);
    assert!(
        add(
            root,
            "mixed",
            vec!["a".into(), "b".into()],
            Bump::Patch,
            "Mixed"
        )
        .is_err()
    );
    assert!(
        changenote::add_generated(
            root,
            vec![("a".into(), Bump::Patch), ("b".into(), Bump::Patch)],
            "Mixed"
        )
        .is_err()
    );
    assert_eq!(snapshot(root), before);
    fs::write(
        root.join(".changenote/manual.md"),
        "---\na: patch\nb: minor\n---\n\nMixed\n",
    )
    .unwrap();
    let before = snapshot(root);
    assert!(version(root).is_err());
    assert_eq!(snapshot(root), before);
}

#[cfg(unix)]
#[test]
fn failed_manager_restores_active_and_retained_notes_and_known_outputs() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project(json!({"ignore":["b"]}));
    let root = dir.path();
    fs::write(
        root.join("a/Cargo.toml"),
        "[package]\nname = \"a\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    fs::write(root.join("a/Cargo.lock"), "original lock").unwrap();
    package_setting(root, "a", "version_file", json!("Cargo.toml"));
    fs::create_dir(root.join("bin")).unwrap();
    let manager = root.join("bin/cargo");
    fs::write(&manager, "#!/bin/sh\nprintf changed > Cargo.lock\nexit 1\n").unwrap();
    fs::set_permissions(&manager, fs::Permissions::from_mode(0o755)).unwrap();
    note(root, "active", &[("a", Bump::Patch)]);
    note(root, "paused", &[("b", Bump::Minor)]);
    let before = snapshot(root);
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .arg("version")
        .env("PATH", root.join("bin"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(snapshot(root), before);
}

#[test]
fn fixed_patterns_expand_before_group_validation_and_keep_sorted_causes() {
    let dir = project(json!({"fixed":[["b","a*"]]}));
    let root = dir.path();
    note(root, "one", &[("a", Bump::Patch)]);
    let releases = changenote::status_with_note_paths(root).unwrap();
    assert_eq!(releases.len(), 2);
    assert_eq!(
        releases[0].fixed_group.as_ref().unwrap().members,
        ["a", "b"]
    );
    version(root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("c/VERSION")).unwrap(),
        "1.0.0\n"
    );
    let dir = project(json!({"fixed":[["***"]]}));
    note(dir.path(), "one", &[("a", Bump::Patch)]);
    assert_eq!(status(dir.path()).unwrap().len(), 4);
    for fixed in [
        json!([["missing*"]]),
        json!([["a*"]]),
        json!([["a", "*"]]),
        json!([["a*", "b"], ["b", "c"]]),
        json!([["[ab]"]]),
        json!([["{a,b}"]]),
        json!([["!a"]]),
        json!([["a/b"]]),
        json!([["A*"]]),
        json!([[""]]),
    ] {
        let dir = project(json!({"fixed":fixed}));
        let before = snapshot(dir.path());
        assert!(version(dir.path()).is_err());
        assert_eq!(snapshot(dir.path()), before);
    }
    let dir = project(json!({"fixed":[["a*","b"]],"linked":[["b","c"]]}));
    assert!(status(dir.path()).is_err());
}

#[test]
fn linked_validation_overflow_and_collision_fail_before_writes() {
    for linked in [
        json!(null),
        json!([[]]),
        json!([["a"]]),
        json!([["a", "a"]]),
        json!([["a", "missing"]]),
        json!([["a", "b"], ["b", "c"]]),
        json!([["a", "b*"]]),
        json!("a"),
    ] {
        let dir = project(json!({"linked":linked}));
        let before = snapshot(dir.path());
        assert!(status(dir.path()).is_err());
        assert!(version(dir.path()).is_err());
        assert_eq!(snapshot(dir.path()), before);
    }
    let dir = project(json!({"linked":[["a","b"]],"fixed":[["b","c"]]}));
    assert!(status(dir.path()).is_err());
    let dir = project(json!({"linked":[["a","b"]]}));
    fs::write(dir.path().join("b/VERSION"), format!("1.0.{}\n", u64::MAX)).unwrap();
    note(dir.path(), "one", &[("a", Bump::Patch)]);
    let before = snapshot(dir.path());
    assert!(version(dir.path()).is_err());
    assert_eq!(snapshot(dir.path()), before);
    let dir = project(json!({"linked":[["a","b"]]}));
    setting(
        dir.path(),
        "packages",
        json!({
            "a":{"path":"a","version_file":"VERSION","tag_prefix":"same/"},
            "b":{"path":"b","version_file":"VERSION","tag_prefix":"same/"}
        }),
    );
    note(dir.path(), "one", &[("a", Bump::Patch), ("b", Bump::Minor)]);
    let before = snapshot(dir.path());
    assert!(version(dir.path()).is_err());
    assert_eq!(snapshot(dir.path()), before);
    let dir = project(json!({"linked":[]}));
    add(dir.path(), "one", vec!["a".into()], Bump::Patch, "Fix").unwrap();
    assert_eq!(status(dir.path()).unwrap().len(), 1);
}
