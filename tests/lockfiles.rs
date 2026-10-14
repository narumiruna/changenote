use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn project(manifest: &str, content: &str, lock: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    fs::write(dir.path().join(manifest), content).unwrap();
    fs::write(dir.path().join(lock), "original lock\n").unwrap();
    fs::write(
        dir.path().join(".changenote/config.json"),
        serde_json::json!({"packages": {"pkg": {"path": ".", "version_file": manifest}}})
            .to_string(),
    )
    .unwrap();
    fs::write(
        dir.path().join(".changenote/change.md"),
        "---\npkg: patch\n---\nFix\n",
    )
    .unwrap();
    dir
}

#[cfg(unix)]
fn fake_tool(root: &Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join(name);
    fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn cli(root: &Path, path: Option<&Path>, args: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_changenote"));
    command.current_dir(root).args(args);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.output().unwrap()
}

#[test]
#[cfg(unix)]
fn dispatches_offline_commands_and_keeps_status_read_only() {
    for (manifest, content, lock, tool, args) in [
        (
            "Cargo.toml",
            "[package]\nversion='1.2.3'\n",
            "Cargo.lock",
            "cargo",
            "update --workspace --offline",
        ),
        (
            "pyproject.toml",
            "[project]\nversion='1.2.3'\n",
            "uv.lock",
            "uv",
            "lock --offline",
        ),
        (
            "package.json",
            "{\"version\":\"1.2.3\"}",
            "package-lock.json",
            "npm",
            "install --package-lock-only --ignore-scripts --offline --no-audit --no-fund",
        ),
    ] {
        let dir = project(manifest, content, lock);
        let bin = fake_tool(
            dir.path(),
            tool,
            &format!("printf '%s\\n' \"$*\" >> calls\nprintf 'updated lock\\n' > {lock}"),
        );
        assert!(cli(dir.path(), Some(&bin), &["status"]).status.success());
        assert!(!dir.path().join("calls").exists());
        let output = cli(dir.path(), Some(&bin), &["version"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("calls")).unwrap(),
            format!("{args}\n")
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(lock)).unwrap(),
            "updated lock\n"
        );
        assert!(!dir.path().join(".changenote/change.md").exists());
        assert!(cli(dir.path(), Some(&bin), &["version"]).status.success());
        assert_eq!(
            fs::read_to_string(dir.path().join("calls")).unwrap(),
            format!("{args}\n")
        );
    }
}

#[test]
#[cfg(unix)]
fn missing_or_failed_manager_restores_inputs_and_keeps_notes() {
    for fail in [false, true] {
        let dir = project(
            "package.json",
            "{\"version\":\"1.2.3\"}",
            "package-lock.json",
        );
        let bin = if fail {
            fake_tool(
                dir.path(),
                "npm",
                "printf 'partial' > package-lock.json\nprintf 'failure' >&2\nexit 1",
            )
        } else {
            let bin = dir.path().join("bin");
            fs::create_dir(&bin).unwrap();
            bin
        };
        fs::write(dir.path().join("CHANGELOG.md"), "old changelog").unwrap();
        fs::write(dir.path().join(".changenote/releases.json"), "old batch").unwrap();
        let output = cli(dir.path(), Some(&bin), &["version"]);
        assert!(!output.status.success());
        assert_eq!(
            fs::read_to_string(dir.path().join("package.json")).unwrap(),
            "{\"version\":\"1.2.3\"}"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("package-lock.json")).unwrap(),
            "original lock\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap(),
            "old changelog"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(".changenote/releases.json")).unwrap(),
            "old batch"
        );
        assert!(dir.path().join(".changenote/change.md").exists());
    }
}

#[test]
#[cfg(unix)]
fn workspace_lock_is_refreshed_once_after_all_manifests() {
    let dir = project(
        "package.json",
        "{\"version\":\"1.2.3\"}",
        "package-lock.json",
    );
    for name in ["a", "b"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(
            dir.path().join(name).join("package.json"),
            "{\"version\":\"1.2.3\"}",
        )
        .unwrap();
    }
    fs::write(dir.path().join(".changenote/config.json"), r#"{"packages":{"a":{"path":"a","version_file":"package.json"},"b":{"path":"b","version_file":"package.json"}},"fixed":[["a","b"]]}"#).unwrap();
    fs::write(
        dir.path().join(".changenote/change.md"),
        "---\na: patch\n---\nFix\n",
    )
    .unwrap();
    let bin = fake_tool(
        dir.path(),
        "npm",
        "IFS= read -r a < a/package.json\nIFS= read -r b < b/package.json\n[ \"$a\" = '{\"version\":\"1.2.4\"}' ] || exit 1\n[ \"$b\" = '{\"version\":\"1.2.4\"}' ] || exit 1\nprintf 'call\\n' >> calls",
    );
    let output = cli(dir.path(), Some(&bin), &["version"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("calls")).unwrap(),
        "call\n"
    );
}

#[test]
#[cfg(unix)]
fn later_manager_failure_restores_successful_lock_updates() {
    let dir = project("pyproject.toml", "[project]\nversion='1.2.3'\n", "uv.lock");
    fs::create_dir(dir.path().join("z")).unwrap();
    fs::write(dir.path().join("z/package.json"), "{\"version\":\"1.2.3\"}").unwrap();
    fs::write(dir.path().join("z/package-lock.json"), "npm original").unwrap();
    fs::write(dir.path().join(".changenote/config.json"), r#"{"packages":{"a":{"path":".","version_file":"pyproject.toml"},"z":{"path":"z","version_file":"package.json"}}}"#).unwrap();
    fs::write(
        dir.path().join(".changenote/change.md"),
        "---\na: patch\nz: patch\n---\nFix\n",
    )
    .unwrap();
    let bin = fake_tool(
        dir.path(),
        "uv",
        "printf 'updated' > uv.lock\nprintf 'ran' > calls",
    );
    fake_tool(
        dir.path(),
        "npm",
        "printf 'partial' > package-lock.json\nexit 1",
    );
    let output = cli(dir.path(), Some(&bin), &["version"]);
    assert!(!output.status.success());
    assert!(dir.path().join("calls").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("uv.lock")).unwrap(),
        "original lock\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("z/package-lock.json")).unwrap(),
        "npm original"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("pyproject.toml")).unwrap(),
        "[project]\nversion='1.2.3'\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("z/package.json")).unwrap(),
        "{\"version\":\"1.2.3\"}"
    );
    assert!(!dir.path().join("CHANGELOG.md").exists());
    assert!(!dir.path().join(".changenote/releases.json").exists());
    assert!(dir.path().join(".changenote/change.md").exists());
}

#[test]
fn plain_version_files_do_not_refresh_adjacent_locks() {
    let dir = project("VERSION", "1.2.3\n", "Cargo.lock");
    changenote::version(dir.path()).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("Cargo.lock")).unwrap(),
        "original lock\n"
    );
}

#[test]
#[cfg(unix)]
fn rejects_lock_symlinks_before_manifest_writes() {
    for external in [false, true] {
        let dir = project(
            "package.json",
            "{\"version\":\"1.2.3\"}",
            "package-lock.json",
        );
        let other = tempfile::tempdir().unwrap();
        let target = if external {
            other.path().join("lock")
        } else {
            dir.path().join("lock")
        };
        fs::write(&target, "unchanged").unwrap();
        fs::remove_file(dir.path().join("package-lock.json")).unwrap();
        std::os::unix::fs::symlink(&target, dir.path().join("package-lock.json")).unwrap();
        assert!(changenote::version(dir.path()).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "unchanged");
        assert_eq!(
            fs::read_to_string(dir.path().join("package.json")).unwrap(),
            "{\"version\":\"1.2.3\"}"
        );
        assert!(dir.path().join(".changenote/change.md").exists());
    }
}

#[test]
fn npm_shrinkwrap_is_rejected_before_writes() {
    let dir = project(
        "package.json",
        "{\"version\":\"1.2.3\"}",
        "package-lock.json",
    );
    fs::write(dir.path().join("npm-shrinkwrap.json"), "unchanged").unwrap();
    assert!(
        changenote::version(dir.path())
            .unwrap_err()
            .to_string()
            .contains("npm-shrinkwrap.json is not supported")
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("package.json")).unwrap(),
        "{\"version\":\"1.2.3\"}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("npm-shrinkwrap.json")).unwrap(),
        "unchanged"
    );
}

#[test]
fn absent_locks_and_historical_profiles_need_no_manager() {
    for args in [
        vec!["version"],
        vec!["version", "--no-lockfiles"],
        vec!["version", "--legacy-output"],
    ] {
        let dir = project(
            "package.json",
            "{\"version\":\"1.2.3\"}",
            "package-lock.json",
        );
        if args.len() == 1 {
            fs::remove_file(dir.path().join("package-lock.json")).unwrap();
        }
        let bin = dir.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let output = cli(dir.path(), Some(&bin), &args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if args.len() == 1 {
            assert!(!dir.path().join("package-lock.json").exists());
        } else {
            assert_eq!(
                fs::read_to_string(dir.path().join("package-lock.json")).unwrap(),
                "original lock\n"
            );
        }
    }
}

#[test]
fn real_uv_and_npm_refresh_existing_locks() {
    for (manifest, content, lock, tool, args) in [
        (
            "pyproject.toml",
            "[project]\nname='pkg'\nversion='1.2.3'\nrequires-python='>=3.12'\n",
            "uv.lock",
            "uv",
            vec!["lock", "--offline"],
        ),
        (
            "package.json",
            r#"{"name":"pkg","version":"1.2.3","scripts":{"preinstall":"touch lifecycle-ran"}}"#,
            "package-lock.json",
            "npm",
            vec![
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--offline",
                "--no-audit",
                "--no-fund",
            ],
        ),
    ] {
        if Command::new(tool).arg("--version").output().is_err() {
            eprintln!("Skipping real {tool} test: tool not installed");
            continue;
        }
        let dir = project(manifest, content, lock);
        fs::remove_file(dir.path().join(lock)).unwrap();
        let output = Command::new(tool)
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        changenote::version(dir.path()).unwrap();
        let updated = fs::read_to_string(dir.path().join(lock)).unwrap();
        if tool == "uv" {
            assert!(updated.contains("version = \"1.2.4\""));
        } else {
            let value: serde_json::Value = serde_json::from_str(&updated).unwrap();
            assert_eq!(value["version"], "1.2.4");
            assert_eq!(value["packages"][""]["version"], "1.2.4");
        }
        assert!(!dir.path().join("lifecycle-ran").exists());
    }
}

#[test]
fn real_cargo_refresh_preserves_dependency_resolution() {
    let dir = project(
        "Cargo.toml",
        "[package]\nname='pkg'\nversion='1.2.3'\nedition='2024'\n",
        "Cargo.lock",
    );
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/lib.rs"), "").unwrap();
    fs::remove_file(dir.path().join("Cargo.lock")).unwrap();
    assert!(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    changenote::version(dir.path()).unwrap();
    let lock = fs::read_to_string(dir.path().join("Cargo.lock")).unwrap();
    assert!(lock.contains("version = \"1.2.4\""));
    assert!(
        Command::new("cargo")
            .args(["check", "--locked", "--offline"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
}
