use changenote::{Bump, Coverage, add, check, check_with_coverage};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(root)
        .args(["-c", "gc.auto=0", "-c", "maintenance.auto=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn commit(root: &Path) {
    git(root, &["add", "--", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-qm",
            "fixture",
        ],
    );
}
fn write(root: &Path, path: &str, data: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, data).unwrap();
}
fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-qb", "main"]);
    write(
        root,
        ".changenote/config.json",
        r#"{"packages":{"pkg":{"path":"./packages/pkg","version_file":"VERSION"},"nested":{"path":"packages/pkg/nested","version_file":"VERSION"}}}"#,
    );
    write(root, "packages/pkg/VERSION", "1.0.0\n");
    write(root, "packages/pkg/nested/VERSION", "1.0.0\n");
    write(root, "packages/pkg/old.txt", "old");
    write(root, "packages/pkg/nested/old.txt", "old");
    add(
        root,
        "old",
        vec!["pkg".into()],
        Bump::Patch,
        "Old pending note",
    )
    .unwrap();
    commit(root);
    git(root, &["branch", "base"]);
    dir
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for item in fs::read_dir(dir).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn cli(root: &Path, args: &[&str]) -> Output {
    let before = snapshot(root);
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(
        snapshot(root),
        before,
        "check must not write even on failure"
    );
    output
}

#[test]
fn pr_level_rules_and_exemptions_are_read_only() {
    let dir = project();
    let root = dir.path();
    assert!(cli(root, &["check", "--since", "base"]).status.success());
    write(root, "packages/pkg/new file 中文.txt", "change");
    commit(root);
    let out = cli(root, &["check", "--since", "base"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["changed_packages"], serde_json::json!(["pkg"]));
    assert_eq!(report["note_paths"], serde_json::json!([]));
    assert_eq!(
        report
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "merge_base",
            "changed_packages",
            "note_paths",
            "outside_paths",
            "skip_reason",
            "passed"
        ]
    );
    let out = cli(
        root,
        &[
            "check",
            "--since",
            "base",
            "--skip-release",
            "--reason",
            "Documentation only",
        ],
    );
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["skip_reason"], "Documentation only");
    for args in [
        vec!["check", "--since", "base", "--skip-release"],
        vec!["check", "--since", "base", "--reason", "Docs"],
        vec![
            "check",
            "--since",
            "base",
            "--skip-release",
            "--reason",
            "  ",
        ],
        vec!["check", "--since", "missing"],
        vec!["check", "--since", "--help"],
    ] {
        assert!(!cli(root, &args).status.success());
    }
    add(
        root,
        "new",
        vec!["nested".into()],
        Bump::Minor,
        "Different package note",
    )
    .unwrap();
    commit(root);
    assert!(cli(root, &["check", "--since", "base"]).status.success());
    assert!(cli(root, &["check", "--since", "HEAD"]).status.success());
    write(root, ".changenote/bad.md", "invalid");
    commit(root);
    let out = cli(
        root,
        &[
            "check",
            "--since",
            "base",
            "--skip-release",
            "--reason",
            "Docs",
        ],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid note"));
}

#[test]
fn modified_and_renamed_notes_count_but_deleted_notes_do_not() {
    for mode in ["modify", "rename", "delete"] {
        let dir = project();
        let root = dir.path();
        write(root, "packages/pkg/old.txt", "new");
        match mode {
            "modify" => write(
                root,
                ".changenote/old.md",
                "---\npkg: patch\n---\n\nUpdated note",
            ),
            "rename" => {
                git(
                    root,
                    &["mv", ".changenote/old.md", ".changenote/new name 中文.md"],
                );
            }
            _ => fs::remove_file(root.join(".changenote/old.md")).unwrap(),
        }
        commit(root);
        assert_eq!(
            cli(root, &["check", "--since", "base"]).status.success(),
            mode != "delete"
        );
        assert_eq!(
            cli(root, &["check", "--since", "base", "--coverage", "package"])
                .status
                .success(),
            mode != "delete"
        );
    }
}

#[test]
fn deepest_package_path_and_component_boundaries_handle_renames_and_deletes() {
    let dir = project();
    let root = dir.path();
    git(
        root,
        &[
            "mv",
            "packages/pkg/nested/old.txt",
            "packages/pkg/nested/new name 中文.txt",
        ],
    );
    write(root, "packages/pkg-sibling/file.txt", "not pkg");
    fs::remove_file(root.join("packages/pkg/old.txt")).unwrap();
    commit(root);
    let out = cli(root, &["check", "--since", "base"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["changed_packages"],
        serde_json::json!(["nested", "pkg"])
    );
    assert_eq!(
        report["outside_paths"],
        serde_json::json!(["packages/pkg-sibling/file.txt"])
    );
    let dir = project();
    let root = dir.path();
    write(root, "packages/pkg/nested/new.txt", "nested only");
    commit(root);
    assert_eq!(
        check(root, "base", true, Some("Docs"))
            .unwrap()
            .changed_packages,
        ["nested"]
    );
    let dir = project();
    let root = dir.path();
    write(root, "README.md", "outside only");
    commit(root);
    assert!(cli(root, &["check", "--since", "base"]).status.success());
}

#[test]
fn strict_coverage_requires_each_changed_package_in_changed_notes() {
    let dir = project();
    let root = dir.path();
    write(root, "packages/pkg/old.txt", "changed");
    add(
        root,
        "unrelated",
        vec!["nested".into()],
        Bump::Patch,
        "Nested change",
    )
    .unwrap();
    commit(root);
    assert!(cli(root, &["check", "--since", "base"]).status.success());
    let out = cli(root, &["check", "--since", "base", "--coverage", "pr"]);
    assert!(out.status.success());
    let default: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(default.get("covered_packages").is_none());
    let out = cli(root, &["check", "--since", "base", "--coverage", "package"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["covered_packages"], serde_json::json!([]));
    assert_eq!(report["uncovered_packages"], serde_json::json!(["pkg"]));
    assert!(
        report.get("report").is_none(),
        "the base report is flattened"
    );

    write(root, "packages/pkg/nested/old.txt", "changed too");
    commit(root);
    let before = snapshot(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert!(!report.report.passed);
    assert_eq!(report.covered_packages, ["nested"]);
    assert_eq!(report.uncovered_packages, ["pkg"]);
    assert_eq!(snapshot(root), before);

    write(
        root,
        ".changenote/old.md",
        "---\npkg: patch\n---\n\nUpdated direct assignment\n",
    );
    commit(root);
    let out = cli(root, &["check", "--since", "base", "--coverage", "package"]);
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["covered_packages"],
        serde_json::json!(["nested", "pkg"])
    );
    assert_eq!(report["uncovered_packages"], serde_json::json!([]));
}

#[test]
fn strict_coverage_accepts_mixed_notes_but_not_fixed_group_propagation() {
    let dir = project();
    let root = dir.path();
    write(
        root,
        ".changenote/config.json",
        r#"{"packages":{"pkg":{"path":"packages/pkg","version_file":"VERSION"},"nested":{"path":"packages/pkg/nested","version_file":"VERSION"}},"fixed":[["pkg","nested"]]}"#,
    );
    write(root, "packages/pkg/old.txt", "changed");
    write(root, "packages/pkg/nested/old.txt", "changed");
    add(
        root,
        "direct",
        vec!["pkg".into()],
        Bump::Patch,
        "Direct change",
    )
    .unwrap();
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert!(!report.report.passed);
    assert_eq!(report.covered_packages, ["pkg"]);
    assert_eq!(report.uncovered_packages, ["nested"]);
    write(
        root,
        ".changenote/direct.md",
        "---\npkg: patch\nnested: minor\n---\n\nBoth packages\n",
    );
    commit(root);
    assert!(
        cli(root, &["check", "--since", "base", "--coverage", "package"])
            .status
            .success()
    );
}

#[test]
fn strict_exemptions_bypass_coverage_but_still_validate() {
    let dir = project();
    let root = dir.path();
    write(root, "packages/pkg/old.txt", "changed");
    commit(root);
    let args = [
        "check",
        "--since",
        "base",
        "--coverage",
        "package",
        "--skip-release",
        "--reason",
        "Docs",
    ];
    let out = cli(root, &args);
    assert!(out.status.success());
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["uncovered_packages"], serde_json::json!(["pkg"]));
    write(
        root,
        ".changenote/invalid.md",
        "---\nunknown: patch\n---\n\nInvalid package\n",
    );
    commit(root);
    let out = cli(root, &args);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown package"));
    fs::remove_file(root.join(".changenote/invalid.md")).unwrap();
    write(root, "packages/pkg/VERSION", "18446744073709551615.0.0\n");
    write(
        root,
        ".changenote/old.md",
        "---\npkg: major\n---\n\nOverflow\n",
    );
    commit(root);
    let out = cli(root, &args);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("version overflow"));
}

#[test]
fn repository_root_paths_and_nested_root_rejection() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo 中文 ");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-qb", "main"]);
    changenote::init(&root, "pkg", "1.0.0".parse().unwrap()).unwrap();
    fs::create_dir(root.join("nested")).unwrap();
    commit(&root);
    assert!(cli(&root, &["check", "--since", "HEAD"]).status.success());
    let output = cli(&root, &["--root", "nested", "check", "--since", "HEAD"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Git repository root"));
    assert!(check(dir.path(), "HEAD", false, None).is_err());
}

#[cfg(unix)]
#[test]
fn check_does_not_execute_git_fsmonitor_hooks() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let root = dir.path();
    let hook = root.join(".git/fsmonitor-hook");
    let sentinel = root.join(".git/executed");
    fs::write(
        &hook,
        format!("#!/bin/sh\nprintf executed > '{}'\n", sentinel.display()),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(root, &["config", "core.fsmonitor", hook.to_str().unwrap()]);
    let output = cli(root, &["check", "--since", "base", "--coverage", "package"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!sentinel.exists());
}

#[test]
fn dirty_config_and_missing_history_fail_without_writes() {
    let dir = project();
    let root = dir.path();
    write(root, "untracked.txt", "untracked");
    assert!(!cli(root, &["check", "--since", "base"]).status.success());
    fs::remove_file(root.join("untracked.txt")).unwrap();
    write(root, "packages/pkg/VERSION", "2.0.0\n");
    assert!(!cli(root, &["check", "--since", "base"]).status.success());
    git(root, &["add", "packages/pkg/VERSION"]);
    assert!(!cli(root, &["check", "--since", "base"]).status.success());
    commit(root);
    write(root, ".changenote/config.json", "bad");
    commit(root);
    assert!(
        !cli(
            root,
            &[
                "check",
                "--since",
                "base",
                "--skip-release",
                "--reason",
                "Docs"
            ]
        )
        .status
        .success()
    );
    let dir = project();
    let root = dir.path();
    git(root, &["checkout", "--orphan", "unrelated"]);
    write(root, "extra", "new root");
    commit(root);
    let out = cli(root, &["check", "--since", "base"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("merge base"));
    assert!(check(root, "HEAD", true, None).is_err());
    assert!(check(root, "HEAD", false, Some("Docs")).is_err());
}
