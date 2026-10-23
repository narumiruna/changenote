use serde_json::json;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn fixture(prefix: Option<serde_json::Value>) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    fs::write(dir.path().join("VERSION"), "1.2.3\n").unwrap();
    let mut package = json!({"path": ".", "version_file": "VERSION"});
    if let Some(prefix) = prefix {
        package["tag_prefix"] = prefix;
    }
    fs::write(
        dir.path().join(".changenote/config.json"),
        json!({"packages": {"pkg": package}}).to_string(),
    )
    .unwrap();
    fs::write(
        dir.path().join(".changenote/change.md"),
        "---\npkg: patch\n---\nFix\n",
    )
    .unwrap();
    dir
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
    git(
        root,
        &["add", "--", ".changenote", "VERSION", "CHANGELOG.md"],
    );
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
}

#[test]
fn literal_prefixes_and_old_defaults() {
    for (prefix, expected) in [
        (None, "v1.2.4"),
        (Some(json!("sdk/v")), "sdk/v1.2.4"),
        (Some(json!("release-")), "release-1.2.4"),
        (Some(json!("")), "1.2.4"),
    ] {
        let dir = fixture(prefix);
        assert_eq!(changenote::status(dir.path()).unwrap()[0].tag, expected);
    }
}

#[test]
fn rejects_null_and_invalid_git_ref_prefixes_without_writes() {
    for prefix in [
        json!(null),
        json!(4),
        json!("/v"),
        json!("-v"),
        json!("sdk//v"),
        json!(".hidden/v"),
        json!("sdk.lock/v"),
        json!("sdk..v"),
        json!("sdk@{v"),
        json!("sdk v"),
        json!("sdk\nv"),
        json!("sdk~v"),
        json!("sdk^v"),
        json!("sdk:v"),
        json!("sdk?v"),
        json!("sdk*v"),
        json!("sdk[v"),
        json!("sdk\\v"),
    ] {
        let dir = fixture(Some(prefix.clone()));
        assert!(changenote::status(dir.path()).is_err(), "{prefix}");
        assert!(changenote::version(dir.path()).is_err(), "{prefix}");
        assert_eq!(
            fs::read_to_string(dir.path().join("VERSION")).unwrap(),
            "1.2.3\n"
        );
    }
}

#[test]
fn prefix_validation_agrees_with_git_full_ref_rules() {
    for prefix in [
        "sdk/v",
        "",
        "@",
        "sdk.lock",
        "nested/ref/v",
        "sdk./v",
        "unicode-é/v",
        ".",
        "/",
        "x//",
        "x.lock/",
        "x../",
        "x@{",
        "x ",
        "x\\t",
        "x\\u{7f}",
        "x~",
        "x^",
        "x:",
        "x?",
        "x*",
        "x[",
        "x\\\\",
    ] {
        let dir = fixture(Some(json!(prefix)));
        let valid = Command::new("git")
            .args(["check-ref-format", &format!("refs/tags/{prefix}1.2.4")])
            .status()
            .unwrap()
            .success();
        assert_eq!(
            changenote::status(dir.path()).is_ok(),
            valid,
            "prefix: {prefix:?}"
        );
    }
}

#[test]
fn planning_does_not_require_git() {
    let dir = fixture(Some(json!("sdk/v")));
    let bin = dir.path().join("empty-bin");
    fs::create_dir(&bin).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(dir.path())
        .env("PATH", bin)
        .arg("status")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("sdk/v1.2.4")
    );
}

#[test]
fn duplicate_final_tags_include_custom_default_and_fixed_group_collisions() {
    for fixed in [false, true] {
        for prefix in ["shared/v", "changenote/b@v"] {
            let dir = fixture(None);
            let root = dir.path();
            for name in ["a", "b"] {
                fs::create_dir(root.join(name)).unwrap();
                fs::write(root.join(name).join("VERSION"), "1.2.3\n").unwrap();
            }
            let mut b = json!({"path": "b", "version_file": "VERSION"});
            if prefix == "shared/v" {
                b["tag_prefix"] = json!(prefix);
            }
            let mut config = json!({"packages": {
                "a": {"path": "a", "version_file": "VERSION", "tag_prefix": prefix}, "b": b
            }});
            if fixed {
                config["fixed"] = json!([["a", "b"]]);
            }
            fs::write(root.join(".changenote/config.json"), config.to_string()).unwrap();
            fs::write(
                root.join(".changenote/change.md"),
                if fixed {
                    "---\na: patch\n---\nFix\n"
                } else {
                    "---\na: patch\nb: patch\n---\nFix\n"
                },
            )
            .unwrap();
            assert!(
                changenote::status(root)
                    .unwrap_err()
                    .to_string()
                    .contains("duplicate release tag")
            );
            assert!(changenote::version(root).is_err());
            assert_eq!(
                fs::read_to_string(root.join("a/VERSION")).unwrap(),
                "1.2.3\n"
            );
        }
    }
}

#[test]
fn custom_tag_creation_is_exact_and_idempotent() {
    let dir = fixture(Some(json!("sdk/v")));
    let root = dir.path();
    changenote::version(root).unwrap();
    git(root, &["init", "-q"]);
    commit(root);
    // Tag identity is fixture-only and is not written to repository configuration.
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .arg("tag")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(git(root, &["tag", "--list"]), "sdk/v1.2.4");
    assert!(changenote::tag(root).unwrap().is_empty());
    assert_eq!(
        git(root, &["cat-file", "-t", "refs/tags/sdk/v1.2.4"]),
        "tag"
    );
}

fn two_packages(prefix: &str) -> TempDir {
    let dir = fixture(Some(json!("v")));
    let root = dir.path();
    fs::create_dir(root.join("other")).unwrap();
    fs::write(root.join("other/VERSION"), "1.2.3\n").unwrap();
    fs::write(
        root.join(".changenote/config.json"),
        json!({"packages": {
            "pkg": {"path":".","version_file":"VERSION","tag_prefix":"v"},
            "other": {"path":"other","version_file":"VERSION","tag_prefix":prefix}
        }})
        .to_string(),
    )
    .unwrap();
    fs::write(
        root.join(".changenote/change.md"),
        "---\npkg: patch\nother: patch\n---\nFix\n",
    )
    .unwrap();
    dir
}

#[test]
fn tag_namespace_conflicts_fail_before_version_writes() {
    let dir = two_packages("v1.2.4/v");
    let root = dir.path();
    assert!(
        changenote::status(root)
            .unwrap_err()
            .to_string()
            .contains("namespace conflict")
    );
    assert!(changenote::version(root).is_err());
    assert_eq!(fs::read_to_string(root.join("VERSION")).unwrap(), "1.2.3\n");
    assert_eq!(
        fs::read_to_string(root.join("other/VERSION")).unwrap(),
        "1.2.3\n"
    );
    assert!(root.join(".changenote/change.md").exists());
    assert!(!root.join(".changenote/releases.json").exists());
}

#[test]
fn saved_tag_namespace_conflicts_fail_before_any_tag_creation() {
    let dir = two_packages("other/v");
    let root = dir.path();
    changenote::version(root).unwrap();
    let config_path = root.join(".changenote/config.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["packages"]["other"]["tag_prefix"] = json!("v1.2.4/v");
    fs::write(config_path, config.to_string()).unwrap();
    let batch_path = root.join(".changenote/releases.json");
    let mut batch: serde_json::Value =
        serde_json::from_slice(&fs::read(&batch_path).unwrap()).unwrap();
    for entry in batch.as_array_mut().unwrap() {
        if entry["package"] == "other" {
            entry["tag"] = json!("v1.2.4/v1.2.4");
        }
    }
    fs::write(batch_path, batch.to_string()).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["add", "--", "other"]);
    commit(root);
    assert!(
        changenote::tag(root)
            .unwrap_err()
            .to_string()
            .contains("namespace conflict")
    );
    assert_eq!(git(root, &["tag", "--list"]), "");
}

#[test]
fn existing_tag_namespaces_fail_preflight_without_creating_tags() {
    for existing in ["sdk", "sdk/v1.2.4/child"] {
        let dir = fixture(Some(json!("sdk/v")));
        let root = dir.path();
        changenote::version(root).unwrap();
        git(root, &["init", "-q"]);
        commit(root);
        git(root, &["tag", existing]);
        assert!(
            changenote::tag(root)
                .unwrap_err()
                .to_string()
                .contains("namespace conflicts")
        );
        assert_eq!(git(root, &["tag", "--list"]), existing);
    }
}

#[test]
fn saved_tags_must_match_prefix_and_cannot_be_duplicated() {
    for invalid in ["v1.2.4", "sdk//v1.2.4", "duplicate"] {
        let dir = fixture(Some(json!("sdk/v")));
        let root = dir.path();
        changenote::version(root).unwrap();
        let path = root.join(".changenote/releases.json");
        let mut saved: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        if invalid == "duplicate" {
            let entry = saved[0].clone();
            saved.as_array_mut().unwrap().push(entry);
        } else {
            saved[0]["tag"] = json!(invalid);
        }
        fs::write(path, saved.to_string()).unwrap();
        git(root, &["init", "-q"]);
        commit(root);
        assert!(changenote::tag(root).is_err());
        assert_eq!(git(root, &["tag", "--list"]), "");
    }
}
