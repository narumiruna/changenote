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
fn setting(root: &Path, key: &str, value: serde_json::Value) {
    let path = root.join(".changenote/config.json");
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config[key] = value;
    fs::write(path, serde_json::to_string(&config).unwrap()).unwrap();
}

#[test]
fn configured_ref_fallback_and_override_are_read_only() {
    let dir = project();
    let root = dir.path();
    assert_eq!(cli(root, &["check"]).status.code(), Some(2));
    assert!(cli(root, &["check", "--since", "HEAD"]).status.success());
    setting(root, "base_branch", "base".into());
    commit(root);
    assert!(cli(root, &["check"]).status.success());
    setting(root, "base_branch", "unavailable".into());
    commit(root);
    assert!(cli(root, &["check", "--since", "HEAD"]).status.success());
    assert!(!cli(root, &["check"]).status.success());
    git(root, &["update-ref", "refs/remotes/origin/base", "base"]);
    setting(root, "base_branch", "refs/remotes/origin/base".into());
    commit(root);
    assert!(cli(root, &["check"]).status.success());
    assert_eq!(
        changenote::configured_base_branch(root).unwrap().as_deref(),
        Some("refs/remotes/origin/base")
    );
}

#[test]
fn configured_ref_rejects_invalid_values_even_with_override() {
    for value in [
        serde_json::Value::Null,
        true.into(),
        3.into(),
        "".into(),
        " ".into(),
        " base".into(),
        "base ".into(),
        "base\n".into(),
        "base\0x".into(),
        serde_json::json!([]),
    ] {
        let dir = project();
        setting(dir.path(), "base_branch", value);
        commit(dir.path());
        assert!(changenote::configured_base_branch(dir.path()).is_err());
        assert!(
            !cli(dir.path(), &["check", "--since", "HEAD"])
                .status
                .success()
        );
    }
}

#[test]
fn file_patterns_filter_coverage_but_not_notes_or_validation() {
    let dir = project();
    let root = dir.path();
    setting(
        root,
        "changed_file_patterns",
        serde_json::json!(["**", "!docs/**", "!tests/**"]),
    );
    commit(root);
    git(root, &["branch", "-f", "base", "HEAD"]);
    write(root, "packages/pkg/docs/a.md", "Docs");
    write(root, "packages/pkg/nested/tests/a.rs", "Tests");
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert!(report.report.passed);
    assert!(report.report.changed_packages.is_empty());
    assert!(report.report.outside_paths.is_empty());
    write(root, "packages/pkg/src/main.rs", "Source");
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert!(!report.report.passed);
    assert_eq!(report.uncovered_packages, ["pkg"]);
    add(root, "new", vec!["pkg".into()], Bump::Patch, "Source note").unwrap();
    commit(root);
    assert!(
        check_with_coverage(root, "base", false, None, Coverage::Package)
            .unwrap()
            .report
            .passed
    );
    // Excluding every path must not bypass invalid versions or old notes.
    setting(
        root,
        "changed_file_patterns",
        serde_json::json!(["never/**"]),
    );
    write(root, "packages/pkg/nested/VERSION", "invalid");
    commit(root);
    assert!(check(root, "base", true, Some("Docs")).is_err());
}

#[test]
fn file_patterns_do_not_reassign_nested_paths_and_check_both_rename_sides() {
    let dir = project();
    let root = dir.path();
    setting(
        root,
        "changed_file_patterns",
        serde_json::json!(["old.txt"]),
    );
    commit(root);
    git(root, &["branch", "-f", "base", "HEAD"]);
    write(root, "packages/pkg/nested/src/new.rs", "excluded");
    commit(root);
    assert!(
        check(root, "base", false, None)
            .unwrap()
            .changed_packages
            .is_empty()
    );
    fs::create_dir_all(root.join("packages/pkg/nested/docs")).unwrap();
    fs::rename(
        root.join("packages/pkg/old.txt"),
        root.join("packages/pkg/nested/docs/moved.txt"),
    )
    .unwrap();
    commit(root);
    assert_eq!(
        check(root, "base", false, None).unwrap().changed_packages,
        ["pkg"]
    );
    fs::remove_file(root.join("packages/pkg/nested/old.txt")).unwrap();
    commit(root);
    assert_eq!(
        check(root, "base", false, None).unwrap().changed_packages,
        ["nested", "pkg"]
    );
}

#[test]
fn root_patterns_exclude_note_paths_without_hiding_changed_notes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-qb", "main"]);
    changenote::init(root, "pkg", semver::Version::new(1, 0, 0)).unwrap();
    setting(root, "changed_file_patterns", serde_json::json!(["src/**"]));
    commit(root);
    git(root, &["branch", "base"]);
    write(root, "src/main.rs", "Source");
    add(root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").unwrap();
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert!(report.report.passed);
    assert_eq!(
        report.report.note_paths,
        [PathBuf::from(".changenote/fix.md")]
    );
    assert_eq!(changenote::status(root).unwrap().len(), 1);
    for value in [
        serde_json::Value::Null,
        serde_json::json!([]),
        serde_json::json!(["!src/**"]),
        true.into(),
    ] {
        setting(root, "changed_file_patterns", value);
        commit(root);
        assert!(check(root, "base", true, Some("Docs")).is_err());
    }
}

#[test]
fn linked_membership_does_not_cover_a_changed_package() {
    let dir = project();
    let root = dir.path();
    setting(root, "linked", serde_json::json!([["pkg", "nested"]]));
    commit(root);
    git(root, &["branch", "-f", "base", "HEAD"]);
    write(root, "packages/pkg/new.rs", "Source");
    write(root, "packages/pkg/nested/new.rs", "Source");
    add(root, "one", vec!["pkg".into()], Bump::Minor, "Fix").unwrap();
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert_eq!(report.uncovered_packages, ["nested"]);
}

#[test]
fn paused_packages_still_require_direct_changed_notes() {
    let dir = project();
    let root = dir.path();
    setting(root, "ignore", serde_json::json!(["pkg"]));
    commit(root);
    git(root, &["branch", "-f", "base", "HEAD"]);
    write(root, "packages/pkg/new.rs", "Source");
    commit(root);
    let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
    assert_eq!(report.uncovered_packages, ["pkg"]);
    add(
        root,
        "paused-new",
        vec!["pkg".into()],
        Bump::Minor,
        "Paused change",
    )
    .unwrap();
    commit(root);
    assert!(
        check_with_coverage(root, "base", false, None, Coverage::Package)
            .unwrap()
            .report
            .passed
    );
    assert!(changenote::status(root).unwrap().is_empty());
}

#[test]
fn filtering_all_paths_does_not_bypass_validation_or_exemptions() {
    for failure in ["note", "version", "dirty", "history", "config"] {
        let dir = project();
        let root = dir.path();
        setting(
            root,
            "changed_file_patterns",
            serde_json::json!(["never/**"]),
        );
        commit(root);
        git(root, &["branch", "-f", "base", "HEAD"]);
        match failure {
            "note" => write(
                root,
                ".changenote/bad.md",
                "---\nmissing: patch\n---\n\nBad\n",
            ),
            "version" => write(root, "packages/pkg/VERSION", "not a version"),
            "dirty" => write(root, "untracked.txt", "Dirty"),
            "config" => setting(root, "changed_file_patterns", serde_json::json!([])),
            "history" => {
                git(root, &["checkout", "--orphan", "unrelated"]);
                write(root, "outside.txt", "New root");
            }
            _ => unreachable!(),
        }
        if failure != "dirty" {
            commit(root);
        }
        for coverage in ["pr", "package"] {
            let output = cli(
                root,
                &[
                    "check",
                    "--since",
                    "base",
                    "--coverage",
                    coverage,
                    "--skip-release",
                    "--reason",
                    "Docs",
                ],
            );
            assert!(!output.status.success(), "{failure}: {coverage}");
            assert!(
                output.stdout.is_empty(),
                "{failure}: validation is not a coverage report"
            );
        }
    }
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
