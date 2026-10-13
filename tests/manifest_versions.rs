use changenote::{Bump, add, status, tag, version};
use semver::Version;
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn project(filename: &str, content: &str) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    fs::write(dir.path().join(filename), content).unwrap();
    fs::write(
        dir.path().join(".changenote/config.json"),
        serde_json::json!({"packages": {"pkg": {"path": ".", "version_file": filename}}})
            .to_string(),
    )
    .unwrap();
    dir
}

fn note(root: &Path) {
    add(
        root,
        "change",
        vec!["pkg".into()],
        Bump::Minor,
        "New feature",
    )
    .unwrap();
}

#[test]
fn reads_and_updates_manifest_versions() {
    for (filename, before, after) in [
        (
            "Cargo.toml",
            "# Package\n[package]\nname = 'pkg'\nversion = \"1.2.3\" # release\n\n[dependencies]\nother = '1.2.3'\n",
            "# Package\n[package]\nname = 'pkg'\nversion = \"1.3.0\" # release\n\n[dependencies]\nother = '1.2.3'\n",
        ),
        (
            "pyproject.toml",
            "[project]\nname = 'pkg'\nversion = \"1.2.3\" # release\ndependencies = ['other==1.2.3']\n\n[tool.example]\nversion = '1.2.3'\n",
            "[project]\nname = 'pkg'\nversion = \"1.3.0\" # release\ndependencies = ['other==1.2.3']\n\n[tool.example]\nversion = '1.2.3'\n",
        ),
        (
            "package.json",
            "{\n  \"name\": \"pkg\",\n  \"version\": \"1.2.3\",\n  \"dependencies\": {\n    \"other\": \"1.2.3\"\n  }\n}\n",
            "{\n  \"name\": \"pkg\",\n  \"version\": \"1.3.0\",\n  \"dependencies\": {\n    \"other\": \"1.2.3\"\n  }\n}\n",
        ),
    ] {
        let dir = project(filename, before);
        note(dir.path());
        let releases = status(dir.path()).unwrap();
        assert_eq!(releases[0].old_version, Version::new(1, 2, 3));
        assert_eq!(releases[0].version, Version::new(1, 3, 0));
        assert_eq!(
            fs::read_to_string(dir.path().join(filename)).unwrap(),
            before
        );
        fs::write(dir.path().join("untouched.lock"), "unchanged").unwrap();
        let releases = version(dir.path()).unwrap();
        assert_eq!(releases[0].tag, "v1.3.0");
        assert_eq!(
            fs::read_to_string(dir.path().join(filename)).unwrap(),
            after
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("untouched.lock")).unwrap(),
            "unchanged"
        );
        assert!(!dir.path().join(".changenote/change.md").exists());
        assert!(version(dir.path()).unwrap().is_empty());
    }
}

#[test]
fn rejects_invalid_or_indirect_manifest_versions_without_writes() {
    for (filename, content) in [
        ("Cargo.toml", "[package]\nname = 'pkg'\n"),
        (
            "Cargo.toml",
            "[package]\nversion.workspace = true\n[workspace.package]\nversion = '1.2.3'\n",
        ),
        ("Cargo.toml", "[package]\nversion = 123\n"),
        ("Cargo.toml", "[package]\nversion = '1.2.3-beta.1'\n"),
        ("Cargo.toml", "[package"),
        ("pyproject.toml", "[project]\ndynamic = ['version']\n"),
        (
            "pyproject.toml",
            "[project]\nversion = '1.2.3'\ndynamic = ['version']\n",
        ),
        ("pyproject.toml", "[tool.poetry]\nversion = '1.2.3'\n"),
        ("pyproject.toml", "[project]\nversion = '1.2.3+build'\n"),
        ("package.json", "{\"name\":\"pkg\"}"),
        ("package.json", "{\"version\":123}"),
        ("package.json", "{\"version\":\"banana\"}"),
        ("package.json", "{\"version\":\"1.2.3-beta.1\"}"),
        ("package.json", "{"),
    ] {
        let dir = project(filename, content);
        fs::write(
            dir.path().join(".changenote/change.md"),
            "---\npkg: minor\n---\nChange\n",
        )
        .unwrap();
        assert!(status(dir.path()).is_err(), "accepted {content}");
        assert!(version(dir.path()).is_err(), "accepted {content}");
        assert_eq!(
            fs::read_to_string(dir.path().join(filename)).unwrap(),
            content
        );
        assert!(dir.path().join(".changenote/change.md").exists());
        assert!(!dir.path().join("CHANGELOG.md").exists());
        assert!(!dir.path().join(".changenote/releases.json").exists());
    }
}

#[test]
fn mixed_language_monorepo_updates_only_noted_packages() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    let mut packages = serde_json::Map::new();
    for (name, filename, content) in [
        ("python", "pyproject.toml", "[project]\nversion = '0.1.0'\n"),
        ("rust", "Cargo.toml", "[package]\nversion = '0.1.0'\n"),
        ("node", "package.json", "{\"version\":\"0.1.0\"}\n"),
        ("go", "VERSION", "0.1.0\n"),
    ] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join(filename), content).unwrap();
        packages.insert(
            name.into(),
            serde_json::json!({"path": name, "version_file": filename}),
        );
    }
    fs::write(
        dir.path().join(".changenote/config.json"),
        serde_json::json!({"packages": packages}).to_string(),
    )
    .unwrap();
    add(
        dir.path(),
        "change",
        vec!["python".into(), "go".into()],
        Bump::Patch,
        "Fix",
    )
    .unwrap();
    let releases = version(dir.path()).unwrap();
    assert_eq!(releases.len(), 2);
    assert_eq!(releases[0].tag, "changenote/go@v0.1.1");
    assert_eq!(releases[1].tag, "changenote/python@v0.1.1");
    assert_eq!(
        fs::read_to_string(dir.path().join("go/VERSION")).unwrap(),
        "0.1.1\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("rust/Cargo.toml")).unwrap(),
        "[package]\nversion = '0.1.0'\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("node/package.json")).unwrap(),
        "{\"version\":\"0.1.0\"}\n"
    );
    assert!(!dir.path().join("rust/CHANGELOG.md").exists());
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn tag_validates_manifest_version() {
    let dir = project("Cargo.toml", "[package]\nversion = '1.2.3'\n");
    git(dir.path(), &["init", "--initial-branch=main"]);
    note(dir.path());
    version(dir.path()).unwrap();
    git(
        dir.path(),
        &["add", ".changenote", "Cargo.toml", "CHANGELOG.md"],
    );
    git(dir.path(), &["commit", "-m", "chore: release"]);
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(dir.path())
        .arg("tag")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(tag(dir.path()).unwrap().is_empty());
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nversion = '1.4.0'\n",
    )
    .unwrap();
    git(dir.path(), &["add", "Cargo.toml"]);
    git(dir.path(), &["commit", "-m", "chore: change version"]);
    let error = tag(dir.path()).unwrap_err().to_string();
    assert!(error.contains("release version does not match version file"));
}
