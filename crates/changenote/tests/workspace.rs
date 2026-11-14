use changenote::{Bump, Coverage, check_with_coverage};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn write(root: &Path, name: &str, content: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

fn project() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/changenote\"]\nresolver = \"3\"\n",
    );
    write(
        root,
        "crates/changenote/Cargo.toml",
        "[package]\nname = \"changenote\"\nversion = \"1.2.3\"\nedition = \"2024\"\n",
    );
    write(root, "crates/changenote/src/lib.rs", "// Fixture\n");
    write(root, "python/changenote/__init__.py", "# Fixture\n");
    write(
        root,
        "Cargo.lock",
        "version = 4\n\n[[package]]\nname = \"changenote\"\nversion = \"1.2.3\"\n",
    );
    write(
        root,
        ".changenote/config.json",
        r#"{"packages":{"changenote":{"path":".","version_file":"crates/changenote/Cargo.toml"}}}"#,
    );
    dir
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn commit(root: &Path) {
    git(root, &["add", "--", "."]);
    git(root, &["commit", "-qm", "test: update fixture"]);
}

#[test]
fn nested_manifest_uses_root_release_outputs_and_shared_lockfile() {
    let dir = project();
    let root = dir.path();
    changenote::add(
        root,
        "fix",
        vec!["changenote".into()],
        Bump::Patch,
        "Fix workspace builds.",
    )
    .unwrap();
    let manifest = fs::read(root.join("crates/changenote/Cargo.toml")).unwrap();
    let lock = fs::read(root.join("Cargo.lock")).unwrap();
    let plan = changenote::status(root).unwrap();
    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].package, "changenote");
    assert_eq!(plan[0].version.to_string(), "1.2.4");
    assert_eq!(plan[0].tag, "v1.2.4");
    assert_eq!(
        fs::read(root.join("crates/changenote/Cargo.toml")).unwrap(),
        manifest
    );
    assert_eq!(fs::read(root.join("Cargo.lock")).unwrap(), lock);
    assert!(!root.join("CHANGELOG.md").exists());

    // Release writes affect only this disposable fixture, never the checkout.
    changenote::version(root).unwrap();
    let manifest = fs::read_to_string(root.join("crates/changenote/Cargo.toml")).unwrap();
    let lock = fs::read_to_string(root.join("Cargo.lock")).unwrap();
    assert!(manifest.contains("version = \"1.2.4\""));
    assert!(lock.contains("version = \"1.2.4\""));
    assert!(!root.join("crates/changenote/Cargo.lock").exists());
    assert!(!root.join("crates/changenote/CHANGELOG.md").exists());
    assert!(
        fs::read_to_string(root.join("CHANGELOG.md"))
            .unwrap()
            .contains("## 1.2.4")
    );
    assert!(!root.join(".changenote/fix.md").exists());
    let batch: Value =
        serde_json::from_slice(&fs::read(root.join(".changenote/releases.json")).unwrap()).unwrap();
    assert_eq!(batch[0]["package"], "changenote");
    assert_eq!(batch[0]["version"], "1.2.4");
    assert!(changenote::status(root).unwrap().is_empty());
}

#[test]
fn root_release_owner_requires_notes_for_rust_and_python_changes() {
    for path in [
        "crates/changenote/src/lib.rs",
        "python/changenote/__init__.py",
    ] {
        let dir = project();
        let root = dir.path();
        git(root, &["init", "-qb", "main"]);
        commit(root);
        git(root, &["branch", "base"]);
        write(root, path, "// Changed fixture\n");
        commit(root);
        let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
        assert!(!report.report.passed);
        assert_eq!(report.report.changed_packages, ["changenote"]);
        assert_eq!(report.uncovered_packages, ["changenote"]);
        assert!(report.report.outside_paths.is_empty());
        changenote::add(
            root,
            "fix",
            vec!["changenote".into()],
            Bump::Patch,
            "Fix workspace builds.",
        )
        .unwrap();
        commit(root);
        let report = check_with_coverage(root, "base", false, None, Coverage::Package).unwrap();
        assert!(report.report.passed);
        assert!(report.uncovered_packages.is_empty());
    }
}
