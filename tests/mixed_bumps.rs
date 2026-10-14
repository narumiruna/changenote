use changenote::{Bump, add, add_with_bumps, parse_note, status, version};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join(".changenote")).unwrap();
    for name in ["a", "b", "c"] {
        fs::create_dir(root.join(name)).unwrap();
        fs::write(root.join(name).join("VERSION"), "1.0.0\n").unwrap();
    }
    fs::write(root.join(".changenote/config.json"), r#"{"packages":{"a":{"path":"a","version_file":"VERSION"},"b":{"path":"b","version_file":"VERSION"},"c":{"path":"c","version_file":"VERSION"}}}"#).unwrap();
    dir
}
fn cli(root: &Path, args: &[&str]) -> bool {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn cli_mixed_bumps_produce_one_note_and_group_per_package() {
    let dir = project();
    let root = dir.path();
    assert!(cli(
        root,
        &[
            "add",
            "shared",
            "--minor",
            "a",
            "--patch",
            "b",
            "--patch",
            "c",
            "--patch",
            "b",
            "-m",
            "Shared\n\n- detail\n\n```sh\nrun\n```"
        ]
    ));
    let text = fs::read_to_string(root.join(".changenote/shared.md")).unwrap();
    let note = parse_note(&text).unwrap();
    assert_eq!(note.packages["a"], Bump::Minor);
    assert_eq!(note.packages["b"], Bump::Patch);
    assert_eq!(note.packages.len(), 3);
    let plan = status(root).unwrap();
    assert_eq!(plan[0].version.to_string(), "1.1.0");
    assert_eq!(plan[1].version.to_string(), "1.0.1");
    version(root).unwrap();
    for (name, heading) in [("a", "Minor"), ("b", "Patch"), ("c", "Patch")] {
        let log = fs::read_to_string(root.join(name).join("CHANGELOG.md")).unwrap();
        assert!(log.contains(&format!("### {heading} Changes")));
        assert!(log.contains("- Shared\n  \n  - detail\n  \n  ```sh\n  run\n  ```"));
        assert!(!log.contains("Major Changes"));
    }
}

#[test]
fn old_api_and_cli_remain_supported_and_failures_do_not_write() {
    let dir = project();
    let root = dir.path();
    add(root, "api", vec!["a".into()], Bump::Patch, "Fix").unwrap();
    assert!(cli(
        root,
        &[
            "add",
            "old",
            "-p",
            "a",
            "-p",
            "b",
            "--bump",
            "minor",
            "-m",
            "Old syntax"
        ]
    ));
    assert!(cli(root, &["add", "major", "--major", "c", "-m", "Major"]));
    for args in [
        vec!["--minor", "a", "--patch", "a"],
        vec!["--patch", "unknown"],
        vec!["-p", "a", "--bump", "patch", "--minor", "b"],
        vec!["--bump", "patch"],
        vec!["-p", "a"],
        vec![],
    ] {
        let mut command = vec!["add", "invalid", "-m", "Fix"];
        command.extend(args);
        assert!(!cli(root, &command));
        assert!(!root.join(".changenote/invalid.md").exists());
    }
    for reserved in ["README", "AGENTS", "CLAUDE", "GEMINI"] {
        assert!(!cli(root, &["add", reserved, "--patch", "a", "-m", "Fix"]));
        assert!(!root.join(format!(".changenote/{reserved}.md")).exists());
    }
    let before = fs::read(root.join(".changenote/api.md")).unwrap();
    assert!(!cli(
        root,
        &["add", "api", "--minor", "a", "-m", "Overwrite"]
    ));
    assert_eq!(fs::read(root.join(".changenote/api.md")).unwrap(), before);
    assert!(
        add_with_bumps(
            root,
            "invalid",
            vec![("a".into(), Bump::Patch), ("a".into(), Bump::Major)],
            "Fix"
        )
        .is_err()
    );
    assert!(!root.join(".changenote/invalid.md").exists());
}
