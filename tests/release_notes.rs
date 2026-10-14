use changenote::{Bump, add, init, release_notes, version};
use semver::Version;
use std::{fs, process::Command};

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), "pkg", Version::new(1, 0, 0)).unwrap();
    dir
}

#[test]
fn exact_entry_preserves_markdown_and_ignores_code_headings() {
    let dir = project();
    let body = "### Minor Changes\n\n- [API](https://example.com)\n  - Nested list\n  \n  ```md\n  ## 9.0.0\n  ```\n\n~~~md\n## 2.0.0\n~~~\n\n    ## 3.0.0\n";
    for newline in ["\n", "\r\n"] {
        let text = format!(
            "# Changelog\n\nIntro\n\n```md\n## 1.1.0\n```\n\n## 1.1.0\n\n{body}\n## 1.0.0\n\nOld\n"
        )
        .replace('\n', newline);
        fs::write(dir.path().join("CHANGELOG.md"), text).unwrap();
        let expected = format!("{}\n", body.trim_end_matches('\n').replace('\n', newline));
        assert_eq!(
            release_notes(dir.path(), "pkg", &Version::new(1, 1, 0)).unwrap(),
            expected
        );
    }
}

#[test]
fn missing_empty_duplicate_and_unknown_entries_fail_without_output() {
    let dir = project();
    for text in [
        "# Changelog\n",
        "## 1.1.0\n\n \n",
        "## 1.1.0\nFirst\n## 1.1.0\nSecond\n",
    ] {
        fs::write(dir.path().join("CHANGELOG.md"), text).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
            .args(["release-notes", "--package", "pkg", "--version", "1.1.0"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap(),
            text
        );
    }
    fs::remove_file(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(release_notes(dir.path(), "pkg", &Version::new(1, 1, 0)).is_err());
    assert!(release_notes(dir.path(), "unknown", &Version::new(1, 1, 0)).is_err());
}

#[test]
fn generated_groups_use_note_bumps_and_saved_batch_has_only_metadata() {
    let dir = project();
    add(
        dir.path(),
        "a",
        vec!["pkg".into()],
        Bump::Patch,
        "Fix\n\n```rs\nrun();\n```",
    )
    .unwrap();
    add(dir.path(), "b", vec!["pkg".into()], Bump::Minor, "Feature").unwrap();
    assert_eq!(changenote::status(dir.path()).unwrap()[0].notes.len(), 2);
    version(dir.path()).unwrap();
    assert_eq!(
        release_notes(dir.path(), "pkg", &Version::new(1, 1, 0)).unwrap(),
        "### Minor Changes\n\n- Feature\n\n### Patch Changes\n\n- Fix\n  \n  ```rs\n  run();\n  ```\n"
    );
    let batch: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".changenote/releases.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        batch,
        serde_json::json!([{"package":"pkg","old_version":"1.0.0","version":"1.1.0","bump":"minor","tag":"v1.1.0"}])
    );
}

#[test]
fn cli_reads_exact_version_with_root_and_writes_nothing() {
    let dir = project();
    let log = "# Changelog\nIntro\n## 2.0.0\nNew\n## 1.0.0\nOld\n";
    fs::write(dir.path().join("CHANGELOG.md"), log).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .arg("--root")
        .arg(dir.path())
        .args(["release-notes", "--package", "pkg", "--version", "1.0.0"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"Old\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap(),
        log
    );
    assert!(!dir.path().join(".changenote/releases.json").exists());
}

#[cfg(unix)]
#[test]
fn changelog_symlinks_are_rejected() {
    let dir = project();
    fs::write(dir.path().join("real.md"), "## 1.1.0\nBody\n").unwrap();
    std::os::unix::fs::symlink("real.md", dir.path().join("CHANGELOG.md")).unwrap();
    assert!(release_notes(dir.path(), "pkg", &Version::new(1, 1, 0)).is_err());
}
