//! All six policies in one mixed-language release, without language runtimes.
use changenote::{Bump, Coverage, add_with_bumps, check_with_coverage, status};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn git(root: &Path, args: &[&str]) {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn commit(root: &Path) {
    git(root, &["add", "--", "."]);
    git(root, &["commit", "-qm", "fixture"]);
}

#[test]
fn all_policies_preserve_mixed_sources_and_pause_notes_without_external_tools() {
    for enabled in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root,".changenote/config.json",&json!({
            "packages":{
                "go-api":{"path":"go-api","version_file":"VERSION","version_outputs":["COPY"]},
                "go-lib":{"path":"go-lib","version_file":"VERSION"},
                "rust":{"path":"rust","version_file":"Cargo.toml","version_outputs":["VERSION"]},
                "python":{"path":"python","version_file":"pyproject.toml","version_outputs":["VERSION"]},
                "node":{"path":"node","version_file":"package.json","version_outputs":["VERSION"]}
            },
            "fixed":[["go-*"]], "linked":[["rust","python"]], "ignore":["python"],
            "base_branch":"comparison", "changed_file_patterns":["src/**"],
            "changelog":{"enabled":enabled,"note_template":"{package}@{version} [{bump}]\n\n{summary}"}
        }).to_string());
        write(root, "go-api/VERSION", "1.0.0\n");
        write(root, "go-api/COPY", "1.0.0\n");
        write(root, "go-api/go.mod", "module example.com/api\n\ngo 1.24\n");
        write(root, "go-lib/VERSION", "1.1.0\n");
        write(
            root,
            "rust/Cargo.toml",
            "[package]\nname = \"rust\"\nversion = \"2.0.0\" # Keep\n",
        );
        write(root, "rust/VERSION", "2.0.0\n");
        write(
            root,
            "python/pyproject.toml",
            "[project]\nname = \"python\"\nversion = \"3.0.0\"\n",
        );
        write(root, "python/VERSION", "3.0.0\n");
        write(root, "python/CHANGELOG.md", "Paused history\n");
        write(
            root,
            "node/package.json",
            "{ \"name\": \"node\", \"version\": \"4.0.0\", \"keep\": 1e+02 }\n",
        );
        write(root, "node/VERSION", "4.0.0\n");
        git(root, &["init", "-qb", "main"]);
        commit(root);
        git(root, &["branch", "comparison"]);
        for name in ["go-api", "rust", "python", "node"] {
            write(root, &format!("{name}/src/change.txt"), "Source change");
        }
        write(root, "go-lib/docs/guide.md", "Excluded docs");
        add_with_bumps(
            root,
            "active",
            vec![
                ("go-api".into(), Bump::Patch),
                ("rust".into(), Bump::Patch),
                ("node".into(), Bump::Minor),
            ],
            "Shared {package}\n\n```text\nLiteral {version}\n```",
        )
        .unwrap();
        add_with_bumps(
            root,
            "paused",
            vec![("python".into(), Bump::Minor)],
            "Paused change",
        )
        .unwrap();
        commit(root);
        let report =
            check_with_coverage(root, "comparison", false, None, Coverage::Package).unwrap();
        assert!(report.report.passed);
        assert_eq!(
            report.report.changed_packages,
            ["go-api", "node", "python", "rust"]
        );
        for args in [
            vec!["check", "--coverage", "package"],
            vec!["check", "--since", "HEAD", "--coverage", "package"],
        ] {
            assert!(
                Command::new(env!("CARGO_BIN_EXE_changenote"))
                    .current_dir(root)
                    .args(args)
                    .output()
                    .unwrap()
                    .status
                    .success()
            );
        }
        let paused = fs::read(root.join(".changenote/paused.md")).unwrap();
        let python = fs::read(root.join("python/pyproject.toml")).unwrap();
        let plan = status(root).unwrap();
        assert_eq!(plan.len(), 4);
        assert_eq!(
            plan.iter()
                .find(|r| r.package == "rust")
                .unwrap()
                .version
                .to_string(),
            "3.0.1"
        );
        let result = Command::new(env!("CARGO_BIN_EXE_changenote"))
            .current_dir(root)
            .arg("version")
            .env("PATH", "")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            fs::read(root.join(".changenote/paused.md")).unwrap(),
            paused
        );
        assert_eq!(
            fs::read(root.join("python/pyproject.toml")).unwrap(),
            python
        );
        assert_eq!(
            fs::read_to_string(root.join("python/CHANGELOG.md")).unwrap(),
            "Paused history\n"
        );
        for (file, value) in [
            ("go-api/VERSION", "1.1.1\n"),
            ("go-api/COPY", "1.1.1\n"),
            ("go-lib/VERSION", "1.1.1\n"),
            ("rust/VERSION", "3.0.1\n"),
            ("node/VERSION", "4.1.0\n"),
            ("python/VERSION", "3.0.0\n"),
        ] {
            assert_eq!(fs::read_to_string(root.join(file)).unwrap(), value);
        }
        assert!(
            fs::read_to_string(root.join("rust/Cargo.toml"))
                .unwrap()
                .contains("# Keep")
        );
        assert_eq!(
            fs::read_to_string(root.join("node/package.json")).unwrap(),
            "{ \"name\": \"node\", \"version\": \"4.1.0\", \"keep\": 1e+02 }\n"
        );
        assert!(!root.join(".changenote/active.md").exists());
        assert!(status(root).unwrap().is_empty());
        let batch: Value =
            serde_json::from_slice(&fs::read(root.join(".changenote/releases.json")).unwrap())
                .unwrap();
        assert_eq!(batch.as_array().unwrap().len(), 4);
        assert!(
            batch
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r.as_object().unwrap().len() == 5)
        );
        assert_eq!(root.join("rust/CHANGELOG.md").exists(), enabled);
        if enabled {
            let body = changenote::release_notes(root, "rust", &"3.0.1".parse().unwrap()).unwrap();
            assert!(body.contains("rust@3.0.1 [patch]"));
            assert!(body.contains("Shared {package}"));
        } else {
            assert!(changenote::release_notes(root, "rust", &"3.0.1".parse().unwrap()).is_err());
        }
    }
}
