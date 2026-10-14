use changenote::{Bump, add, init, parse_note, status, tag, version};
use semver::Version;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    init(dir.path(), "pkg-a", Version::new(0, 0, 0)).unwrap();
    dir
}

fn note(root: &Path, name: &str, bump: Bump) {
    add(root, name, vec!["pkg-a".into()], bump, "Initial release").unwrap();
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn commit(root: &Path) {
    git(root, &["add", ".changenote", "VERSION", "CHANGELOG.md"]);
    // Supply identity for this one test command without changing Git configuration.
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-m",
            "chore: release",
        ],
    );
}

#[test]
fn parses_requested_example_and_crlf() {
    let source = "---\n\"pkg-a\": minor\n---\n\nInitial release\n";
    for text in [source.to_owned(), source.replace('\n', "\r\n")] {
        let note = parse_note(&text).unwrap();
        assert_eq!(note.packages["pkg-a"], Bump::Minor);
        assert_eq!(note.summary, "Initial release");
    }
}

#[test]
fn rejects_invalid_notes() {
    for text in [
        "hello",
        "---\npkg-a: minor",
        "---\n{}\n---\ntext",
        "---\npkg-a: banana\n---\ntext",
        "---\npkg-a: minor\n---\n",
        "---\npkg-a: patch\npkg-a: major\n---\ntext",
    ] {
        assert!(parse_note(text).is_err(), "accepted {text:?}");
    }
}

#[test]
fn highest_bump_wins_once_and_status_does_not_write() {
    let dir = project();
    note(dir.path(), "a", Bump::Patch);
    note(dir.path(), "b", Bump::Minor);
    note(dir.path(), "c", Bump::Major);
    let plan = status(dir.path()).unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].version, Version::new(1, 0, 0));
    assert_eq!(plan[0].notes.len(), 3);
    assert_eq!(
        plan[0].note_bumps,
        vec![Bump::Patch, Bump::Minor, Bump::Major]
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("VERSION")).unwrap(),
        "0.0.0\n"
    );
    assert!(dir.path().join(".changenote/a.md").exists());
}

#[test]
fn status_note_bumps_are_opt_in() {
    let dir = project();
    note(dir.path(), "a", Bump::Patch);
    note(dir.path(), "b", Bump::Minor);
    for include in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_changenote"));
        command.current_dir(dir.path()).arg("status");
        if include {
            command.arg("--include-note-bumps");
        }
        let output = command.output().unwrap();
        assert!(output.status.success());
        let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        if include {
            assert_eq!(plan[0]["note_bumps"], serde_json::json!(["patch", "minor"]));
        } else {
            assert!(plan[0].get("note_bumps").is_none());
        }
    }
    assert!(dir.path().join(".changenote/a.md").exists());
    assert!(!dir.path().join(".changenote/releases.json").exists());
}

#[test]
fn consumes_notes_and_preserves_previous_changelog() {
    let dir = project();
    fs::write(dir.path().join("CHANGELOG.md"), "## 0.0.0\n\nOld entry\n").unwrap();
    fs::write(dir.path().join(".changenote/README.md"), "Documentation").unwrap();
    note(dir.path(), "hello-world", Bump::Minor);
    let releases = version(dir.path()).unwrap();
    assert_eq!(releases[0].tag, "v0.1.0");
    assert_eq!(
        fs::read_to_string(dir.path().join("VERSION")).unwrap(),
        "0.1.0\n"
    );
    let log = fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(log.starts_with("## 0.1.0\n\n### Minor Changes\n\n- Initial release"));
    assert!(log.contains("Old entry"));
    assert!(!dir.path().join(".changenote/hello-world.md").exists());
    assert!(dir.path().join(".changenote/README.md").exists());
    let saved = fs::read_to_string(dir.path().join(".changenote/releases.json")).unwrap();
    let batch: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(batch[0].get("note_bumps").is_none());
    assert!(version(dir.path()).unwrap().is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join(".changenote/releases.json")).unwrap(),
        saved
    );
}

#[test]
fn invalid_input_leaves_version_and_notes_unchanged() {
    let dir = project();
    note(dir.path(), "valid", Bump::Minor);
    let path = dir.path().join(".changenote/unknown.md");
    fs::write(&path, "---\nmissing: patch\n---\nUnknown\n").unwrap();
    assert!(version(dir.path()).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("VERSION")).unwrap(),
        "0.0.0\n"
    );
    assert!(path.exists());
    assert!(dir.path().join(".changenote/valid.md").exists());
    assert!(!dir.path().join("CHANGELOG.md").exists());
}

#[test]
fn add_does_not_overwrite_or_accept_unknown_packages() {
    let dir = project();
    note(dir.path(), "hello-world", Bump::Minor);
    assert!(
        add(
            dir.path(),
            "hello-world",
            vec!["pkg-a".into()],
            Bump::Patch,
            "Overwrite"
        )
        .is_err()
    );
    assert!(
        add(
            dir.path(),
            "bad",
            vec!["missing".into()],
            Bump::Patch,
            "Unknown"
        )
        .is_err()
    );
    assert!(
        add(
            dir.path(),
            "../bad",
            vec!["pkg-a".into()],
            Bump::Patch,
            "Escape"
        )
        .is_err()
    );
    assert!(
        add(
            dir.path(),
            "README",
            vec!["pkg-a".into()],
            Bump::Patch,
            "Ignored"
        )
        .is_err()
    );
}

#[test]
fn monorepo_versions_only_changed_packages_and_uses_scoped_tags() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    for name in ["pkg-a", "pkg-b"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join("VERSION"), "0.1.0\n").unwrap();
    }
    fs::write(dir.path().join(".changenote/config.json"), r#"{"packages":{"pkg-a":{"path":"pkg-a","version_file":"VERSION"},"pkg-b":{"path":"pkg-b","version_file":"VERSION"}}}"#).unwrap();
    note(dir.path(), "hello-world", Bump::Patch);
    let releases = version(dir.path()).unwrap();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].tag, "changenote/pkg-a@v0.1.1");
    assert_eq!(
        fs::read_to_string(dir.path().join("pkg-b/VERSION")).unwrap(),
        "0.1.0\n"
    );
    assert!(!dir.path().join("pkg-b/CHANGELOG.md").exists());
}

#[test]
fn supports_one_note_for_multiple_packages() {
    let dir = project();
    fs::create_dir(dir.path().join("b")).unwrap();
    fs::write(dir.path().join("b/VERSION"), "1.2.3\n").unwrap();
    fs::write(dir.path().join(".changenote/config.json"), r#"{"packages":{"pkg-a":{"path":".","version_file":"VERSION"},"pkg-b":{"path":"b","version_file":"VERSION"}}}"#).unwrap();
    add(
        dir.path(),
        "both",
        vec!["pkg-a".into(), "pkg-b".into()],
        Bump::Minor,
        "Shared change",
    )
    .unwrap();
    let releases = version(dir.path()).unwrap();
    assert_eq!(releases.len(), 2);
    assert_eq!(releases[1].version, Version::new(1, 3, 0));
}

#[test]
fn rejects_parent_paths_shared_files_and_prereleases() {
    let dir = project();
    let path = dir.path().join(".changenote/config.json");
    for config in [
        r#"{"packages":{"pkg-a":{"path":"..","version_file":"VERSION"}}}"#,
        r#"{"packages":{"pkg-a":{"path":".","version_file":"VERSION"},"pkg-b":{"path":".","version_file":"VERSION"}}}"#,
    ] {
        fs::write(&path, config).unwrap();
        assert!(status(dir.path()).is_err());
    }
    fs::write(
        &path,
        r#"{"packages":{"pkg-a":{"path":".","version_file":"VERSION"}}}"#,
    )
    .unwrap();
    fs::write(dir.path().join("VERSION"), "1.0.0-beta.1\n").unwrap();
    assert!(status(dir.path()).is_err());
}

#[test]
fn git_tags_require_commit_and_are_idempotent_at_same_commit() {
    let dir = project();
    git(dir.path(), &["init", "--initial-branch=main"]);
    note(dir.path(), "hello", Bump::Minor);
    version(dir.path()).unwrap();
    assert!(tag(dir.path()).is_err());
    commit(dir.path());
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .args(["--root", dir.path().to_str().unwrap(), "tag"])
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "v0.1.0");
    assert!(tag(dir.path()).unwrap().is_empty());
    assert_eq!(git(dir.path(), &["cat-file", "-t", "v0.1.0"]), "tag");
    fs::write(dir.path().join("CHANGELOG.md"), "Edited\n").unwrap();
    commit(dir.path());
    assert!(tag(dir.path()).is_err());
}

#[test]
fn cli_reports_errors_and_json() {
    let dir = project();
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .args(["--root", dir.path().to_str().unwrap(), "status"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "[]");
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .args([
            "--root",
            dir.path().to_str().unwrap(),
            "add",
            "bad",
            "-p",
            "missing",
            "--bump",
            "patch",
            "-m",
            "Unknown",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown package")
    );
}

#[cfg(unix)]
#[test]
fn rejects_external_symlinks_before_writes() {
    use std::os::unix::fs::symlink;
    let dir = project();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("VERSION"), "9.0.0\n").unwrap();
    fs::remove_file(dir.path().join("VERSION")).unwrap();
    symlink(outside.path().join("VERSION"), dir.path().join("VERSION")).unwrap();
    assert!(status(dir.path()).is_err());
    assert_eq!(
        fs::read_to_string(outside.path().join("VERSION")).unwrap(),
        "9.0.0\n"
    );
    fs::remove_file(dir.path().join("VERSION")).unwrap();
    fs::write(dir.path().join("VERSION"), "0.0.0\n").unwrap();
    note(dir.path(), "hello", Bump::Minor);
    // A dangling symlink must not create a file outside the repository.
    symlink(
        outside.path().join("missing.md"),
        dir.path().join("CHANGELOG.md"),
    )
    .unwrap();
    assert!(version(dir.path()).is_err());
    assert!(!outside.path().join("missing.md").exists());
    assert!(dir.path().join(".changenote/hello.md").exists());
}
