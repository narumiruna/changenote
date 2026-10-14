use changenote::{
    Bump, Config, Package, Release, add, add_generated, add_with_bumps, load_config, status,
    status_with_note_paths, version, version_with_legacy_output,
};
use semver::Version;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn project(fixed: Value) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    let mut packages = serde_json::Map::new();
    for p in ["a", "b", "c", "d", "e"] {
        fs::create_dir(dir.path().join(p)).unwrap();
        fs::write(dir.path().join(p).join("VERSION"), "1.0.0\n").unwrap();
        packages.insert(p.into(), json!({"path":p,"version_file":"VERSION"}));
    }
    fs::write(
        dir.path().join(".changenote/config.json"),
        json!({"packages":packages,"fixed":fixed}).to_string(),
    )
    .unwrap();
    dir
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(path).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(root, &p, files);
            } else {
                files.insert(p.strip_prefix(root).unwrap().into(), fs::read(p).unwrap());
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}
fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "gc.auto=0", "-c", "maintenance.auto=false"])
        .current_dir(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn note(root: &Path, name: &str, packages: Vec<(String, Bump)>) {
    add_with_bumps(root, name, packages, "Same summary").unwrap();
}

#[test]
fn public_layout_signatures_and_serialization_remain_compatible() {
    let _config = Config {
        schema: None,
        packages: BTreeMap::from([(
            "a".into(),
            Package {
                path: "a".into(),
                version_file: "VERSION".into(),
            },
        )]),
    };
    let release = Release {
        package: "a".into(),
        old_version: Version::new(1, 0, 0),
        version: Version::new(1, 0, 1),
        bump: Bump::Patch,
        tag: "changenote/a@v1.0.1".into(),
        notes: vec!["Same summary".into()],
        note_bumps: vec![Bump::Patch],
    };
    let _add = add as fn(&Path, &str, Vec<String>, Bump, &str) -> anyhow::Result<PathBuf>;
    let _mixed =
        add_with_bumps as fn(&Path, &str, Vec<(String, Bump)>, &str) -> anyhow::Result<PathBuf>;
    let _status = status as fn(&Path) -> anyhow::Result<Vec<Release>>;
    let dir = project(json!([]));
    note(dir.path(), "one", vec![("a".into(), Bump::Patch)]);
    let expected = serde_json::to_string_pretty(&vec![release]).unwrap();
    assert_eq!(
        String::from_utf8(cli(dir.path(), &["status"]).stdout).unwrap(),
        format!("{expected}\n")
    );
    version(dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join(".changenote/releases.json")).unwrap(),
        format!("{expected}\n")
    );
    let single = tempfile::tempdir().unwrap();
    changenote::init(single.path(), "a", Version::new(1, 0, 0)).unwrap();
    add(single.path(), "one", vec!["a".into()], Bump::Patch, "Fix").unwrap();
    assert_eq!(status(single.path()).unwrap()[0].tag, "v1.0.1");
}

#[test]
fn note_paths_align_without_merging_identical_summaries_or_writing() {
    let dir = project(json!([]));
    let root = dir.path();
    note(
        root,
        "z",
        vec![("a".into(), Bump::Minor), ("b".into(), Bump::Patch)],
    );
    note(root, "a", vec![("a".into(), Bump::Patch)]);
    git(root, &["init", "-q"]);
    git(root, &["add", "--", "."]);
    git(root, &["commit", "-qm", "fixture"]);
    let before = snapshot(root);
    for flags in [
        vec![],
        vec!["--include-note-paths"],
        vec!["--include-note-bumps"],
        vec!["--include-note-paths", "--include-note-bumps"],
    ] {
        let mut args = vec!["status"];
        args.extend(&flags);
        let out = cli(root, &args);
        assert!(out.status.success());
        let values: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(values[0]["notes"], json!(["Same summary", "Same summary"]));
        if flags.contains(&"--include-note-paths") {
            assert_eq!(
                values[0]["note_paths"],
                json!([".changenote/a.md", ".changenote/z.md"])
            );
            assert_eq!(values[1]["note_paths"], json!([".changenote/z.md"]));
        } else {
            assert!(values[0].get("note_paths").is_none());
        }
        if flags.contains(&"--include-note-bumps") {
            assert_eq!(values[0]["note_bumps"], json!(["patch", "minor"]));
        } else {
            assert!(values[0].get("note_bumps").is_none());
        }
        assert_eq!(snapshot(root), before);
    }
    fs::write(root.join(".changenote/bad.md"), "bad").unwrap();
    let before = snapshot(root);
    assert!(
        !cli(root, &["status", "--include-note-paths"])
            .status
            .success()
    );
    assert_eq!(snapshot(root), before);
}

#[test]
fn fixed_validation_rejects_invalid_groups_without_writes() {
    for groups in [
        json!([[]]),
        json!([["a"]]),
        json!([["a", "a"]]),
        json!([["a", "b"], ["b", "c"]]),
        json!([["a", "missing"]]),
        json!([["a", "*"]]),
        json!(null),
        json!(["a"]),
    ] {
        let dir = project(groups);
        let before = snapshot(dir.path());
        assert!(load_config(dir.path()).is_err());
        assert!(status(dir.path()).is_err());
        assert!(version(dir.path()).is_err());
        assert_eq!(snapshot(dir.path()), before);
    }
    let dir = project(json!([]));
    let path = dir.path().join(".changenote/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config.as_object_mut().unwrap().remove("fixed");
    fs::write(&path, config.to_string()).unwrap();
    assert!(load_config(dir.path()).is_ok());
    config["unexpected"] = json!(true);
    fs::write(&path, config.to_string()).unwrap();
    assert!(load_config(dir.path()).is_err());
}

#[test]
fn fixed_groups_use_highest_baseline_and_bump_with_direct_provenance() {
    let dir = project(json!([["b", "a"], ["d", "c"]]));
    let root = dir.path();
    fs::write(root.join("b/VERSION"), "2.4.9\n").unwrap();
    note(root, "patch", vec![("b".into(), Bump::Patch)]);
    note(
        root,
        "minor",
        vec![("a".into(), Bump::Minor), ("c".into(), Bump::Patch)],
    );
    let details = status_with_note_paths(root).unwrap();
    assert_eq!(details.len(), 4);
    assert_eq!(details[0].release.old_version.to_string(), "1.0.0");
    assert_eq!(details[1].release.old_version.to_string(), "2.4.9");
    assert_eq!(details[0].release.version.to_string(), "2.5.0");
    assert_eq!(details[1].release.version.to_string(), "2.5.0");
    assert_eq!(details[1].release.note_bumps, [Bump::Patch]);
    assert_eq!(
        details[0].note_paths,
        [PathBuf::from(".changenote/minor.md")]
    );
    assert_eq!(
        details[0].fixed_group.as_ref().unwrap().source_note_paths,
        [
            PathBuf::from(".changenote/minor.md"),
            PathBuf::from(".changenote/patch.md")
        ]
    );
    assert!(details[3].release.notes.is_empty());
    assert!(details[3].note_paths.is_empty());
    let before = snapshot(root);
    assert!(version_with_legacy_output(root, true).is_err());
    assert_eq!(snapshot(root), before);
    version(root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("d/CHANGELOG.md")).unwrap(),
        "## 1.0.1\n\nReleased with its fixed version group; no direct changes.\n\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("e/VERSION")).unwrap(),
        "1.0.0\n"
    );
    assert!(!root.join("e/CHANGELOG.md").exists());
    let before = snapshot(root);
    assert!(version(root).unwrap().is_empty());
    assert_eq!(snapshot(root), before);
    git(root, &["init", "-q"]);
    git(root, &["add", "--", "."]);
    git(root, &["commit", "-qm", "fixture"]);
    let out = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .arg("tag")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8(out.stdout).unwrap().lines().count(), 4);
}

#[test]
fn fixed_groups_manifest_sources_no_notes_and_overflow() {
    let dir = project(json!([["a", "b", "c", "d"]]));
    let root = dir.path();
    let path = root.join(".changenote/config.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for (p, file, text) in [
        ("b", "package.json", "{ \"version\" : \"2.0.0\" }"),
        ("c", "Cargo.toml", "[package]\nversion = \"1.2.0\"\n"),
        ("d", "pyproject.toml", "[project]\nversion = \"1.5.0\"\n"),
    ] {
        config["packages"][p]["version_file"] = json!(file);
        fs::write(root.join(p).join(file), text).unwrap();
    }
    fs::write(&path, config.to_string()).unwrap();
    let before = snapshot(root);
    assert!(version(root).unwrap().is_empty());
    assert_eq!(snapshot(root), before);
    note(root, "fix", vec![("a".into(), Bump::Patch)]);
    version(root).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("b/package.json")).unwrap(),
        "{ \"version\" : \"2.0.1\" }"
    );
    assert!(status_with_note_paths(root).unwrap().is_empty());
    fs::write(root.join("a/VERSION"), format!("1.{}.0\n", u64::MAX)).unwrap();
    note(root, "minor", vec![("a".into(), Bump::Minor)]);
    assert_eq!(status(root).unwrap()[0].version.to_string(), "2.1.0"); // lower member arithmetic is not the group baseline
    fs::write(root.join("a/VERSION"), format!("{}.0.0\n", u64::MAX)).unwrap();
    note(root, "major", vec![("a".into(), Bump::Major)]);
    let before = snapshot(root);
    assert!(version(root).is_err());
    assert_eq!(snapshot(root), before);
}

#[test]
fn inactive_groups_and_noted_independent_packages_stay_independent() {
    let dir = project(json!([["a", "b"], ["c", "d"]]));
    let root = dir.path();
    fs::write(root.join("d/VERSION"), "9.0.0\n").unwrap();
    note(
        root,
        "major",
        vec![("a".into(), Bump::Major), ("e".into(), Bump::Patch)],
    );
    let releases = version(root).unwrap();
    assert_eq!(
        releases
            .iter()
            .map(|r| (r.package.as_str(), r.version.to_string()))
            .collect::<Vec<_>>(),
        [
            ("a", "2.0.0".into()),
            ("b", "2.0.0".into()),
            ("e", "1.0.1".into())
        ]
    );
    assert_eq!(
        fs::read_to_string(root.join("c/VERSION")).unwrap(),
        "1.0.0\n"
    );
    assert_eq!(
        fs::read_to_string(root.join("d/VERSION")).unwrap(),
        "9.0.0\n"
    );
    assert!(!root.join("c/CHANGELOG.md").exists());
    for (text, bump) in [
        (format!("1.0.{}\n", u64::MAX), Bump::Patch),
        (format!("1.{}.0\n", u64::MAX), Bump::Minor),
    ] {
        let overflow = project(json!([["a", "b"]]));
        fs::write(overflow.path().join("b/VERSION"), text).unwrap();
        note(overflow.path(), "overflow", vec![("a".into(), bump)]);
        let before = snapshot(overflow.path());
        assert!(version(overflow.path()).is_err());
        assert_eq!(snapshot(overflow.path()), before);
    }
}

// Linux permits non-UTF-8 filenames; macOS rejects them at file creation.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_paths_fail_only_detailed_json_without_writes() {
    use std::os::unix::ffi::OsStringExt;
    let dir = project(json!([]));
    let path = dir
        .path()
        .join(".changenote")
        .join(std::ffi::OsString::from_vec(b"bad-\xff.md".to_vec()));
    fs::write(path, "---\na: patch\n---\nFix\n").unwrap();
    let before = snapshot(dir.path());
    assert!(cli(dir.path(), &["status"]).status.success());
    let out = cli(dir.path(), &["status", "--include-note-paths"]);
    assert!(!out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("note path is not UTF-8")
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn unnamed_add_validates_and_allocates_distinct_files_in_parallel() {
    let dir = project(json!([]));
    let root = dir.path();
    for args in [
        vec!["add", "--patch", "missing", "-m", "Fix"],
        vec!["add", "--patch", "a", "--minor", "a", "-m", "Fix"],
        vec!["add", "--patch", "a", "-m", " "],
        vec!["add", "-m", "Fix"],
    ] {
        let before = snapshot(root);
        assert!(!cli(root, &args).status.success());
        assert_eq!(snapshot(root), before);
    }
    assert!(
        cli(
            root,
            &["add", "-p", "a", "--bump", "minor", "-m", "Feature"]
        )
        .status
        .success()
    );
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..12)
            .map(|_| s.spawn(|| cli(root, &["add", "--patch", "a", "--minor", "b", "-m", "Fix"])))
            .collect();
        for h in handles {
            let out = h.join().unwrap();
            assert!(out.status.success());
            let path = String::from_utf8(out.stdout).unwrap();
            let name = Path::new(path.trim())
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap();
            assert!(name.starts_with("note-"));
            assert_eq!(name.len(), 37);
        }
    });
    assert_eq!(status(root).unwrap()[0].notes.len(), 13);
    assert!(add_generated(root, vec![], "Fix").is_err());
}
