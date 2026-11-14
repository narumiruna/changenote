use changenote::{Bump, release_notes, status_with_note_sections};
use semver::Version;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use tempfile::TempDir;

const REPOSITORY: &str = "https://github.com/test/repo";

struct Project {
    _temp: TempDir,
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_owned();
        let project = Self { _temp: temp, root };
        project.git(&["init", "-qb", "main"]);
        changenote::init(&project.root, "pkg", Version::new(1, 0, 0)).unwrap();
        project.configure(json!({"source_repository": REPOSITORY}));
        project.commit("initial");
        project
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(&self.root)
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn configure(&self, controls: Value) {
        let path = self.root.join(".changenote/config.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        config["changelog"] = controls;
        fs::write(path, config.to_string()).unwrap();
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "--", "."]);
        self.git(&["commit", "-qm", message]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn note(&self, name: &str, summary: &str) -> String {
        changenote::add(&self.root, name, vec!["pkg".into()], Bump::Patch, summary).unwrap();
        self.commit("add note")
    }

    fn rendered(&self) -> String {
        status_with_note_sections(&self.root).unwrap()[0].note_sections[0].notes[0].clone()
    }

    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_changenote"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
}

fn attributed(summary: &str, commits: &[String]) -> String {
    let mut commits = commits.to_vec();
    commits.sort();
    let links = commits
        .iter()
        .map(|sha| format!("[{}]({REPOSITORY}/commit/{sha})", &sha[..12]))
        .collect::<Vec<_>>()
        .join(", ");
    format!("Sources: {links}\n\n{summary}")
}

#[test]
fn creation_edits_rename_and_recreation_have_exact_stable_sources() {
    let p = Project::new();
    let a = p.note(
        "old",
        "Fix output ([#3](https://github.com/test/repo/pull/3)).",
    );
    let path = p.root.join(".changenote/old.md");
    fs::write(&path, fs::read_to_string(&path).unwrap() + "More detail.\n").unwrap();
    let b = p.commit("edit note");
    p.git(&["mv", ".changenote/old.md", ".changenote/new.md"]);
    let c = p.commit("rename note");
    let summary = "Fix output ([#3](https://github.com/test/repo/pull/3)).\nMore detail.";
    let expected = attributed(summary, &[a, b, c]);
    assert_eq!(p.rendered(), expected);
    fs::write(p.root.join("unrelated"), "change").unwrap();
    p.commit("unrelated");
    assert_eq!(p.rendered(), expected);
    assert_eq!(changenote::status(&p.root).unwrap()[0].notes, [summary]);
    fs::remove_file(p.root.join(".changenote/new.md")).unwrap();
    p.commit("delete old lifecycle");
    let d = p.note("new", "New change");
    assert_eq!(p.rendered(), attributed("New change", &[d]));
}

#[test]
fn feature_merges_use_first_parent_changes_and_release_replay_is_identical() {
    for mode in ["merge", "squash", "rebase"] {
        let p = Project::new();
        let first = p.note("fix", "Fix");
        p.git(&["checkout", "-qb", "feature"]);
        fs::write(
            p.root.join(".changenote/fix.md"),
            "---\npkg: patch\n---\n\nFix\nDetails\n",
        )
        .unwrap();
        let side = p.commit("edit on side branch");
        p.git(&["checkout", "-q", "main"]);
        match mode {
            "merge" => {
                p.git(&["merge", "--no-ff", "-qm", "merge feature", "feature"]);
            }
            "squash" => {
                p.git(&["merge", "--squash", "feature"]);
                p.commit("squash feature");
            }
            _ => {
                p.git(&["cherry-pick", &side]);
            }
        }
        let source = p.git(&["rev-parse", "HEAD"]);
        let expected = attributed("Fix\nDetails", &[first, source]);
        assert_eq!(p.rendered(), expected);
        fs::write(p.root.join("unrelated"), "later base change").unwrap();
        let parent = p.commit("later unrelated commit");
        changenote::version(&p.root).unwrap();
        let changelog = fs::read(p.root.join("CHANGELOG.md")).unwrap();
        let batch = fs::read(p.root.join(".changenote/releases.json")).unwrap();
        let tree = p.commit("release");
        let expected_tree = p.git(&["rev-parse", &format!("{tree}^{{tree}}")]);
        p.git(&["reset", "--hard", &parent]);
        assert_eq!(p.rendered(), expected);
        changenote::version(&p.root).unwrap();
        assert_eq!(fs::read(p.root.join("CHANGELOG.md")).unwrap(), changelog);
        assert_eq!(
            fs::read(p.root.join(".changenote/releases.json")).unwrap(),
            batch
        );
        p.git(&["add", "--", "."]);
        assert_eq!(p.git(&["write-tree"]), expected_tree);
    }
}

#[test]
fn missing_dirty_and_shallow_sources_never_get_false_links_or_partial_writes() {
    for mode in ["untracked", "dirty", "archive", "shallow"] {
        let p = Project::new();
        if mode == "untracked" {
            changenote::add(&p.root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").unwrap();
        } else {
            p.note("fix", "Fix");
            if mode == "dirty" {
                fs::write(
                    p.root.join(".changenote/fix.md"),
                    "---\npkg: patch\n---\n\nChanged\n",
                )
                .unwrap();
            } else if mode == "archive" {
                fs::remove_dir_all(p.root.join(".git")).unwrap();
            } else {
                let head = p.git(&["rev-parse", "HEAD"]);
                fs::write(p.root.join(".git/shallow"), format!("{head}\n")).unwrap();
            }
        }
        let summary = if mode == "dirty" { "Changed" } else { "Fix" };
        assert_eq!(p.rendered(), summary, "{mode}");
        let note = fs::read(p.root.join(".changenote/fix.md")).unwrap();
        let version = fs::read(p.root.join("VERSION")).unwrap();
        let output = p.cli(&["version"]);
        assert!(!output.status.success(), "{mode}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("cannot prove note sources"));
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read(p.root.join("VERSION")).unwrap(), version);
        assert_eq!(fs::read(p.root.join(".changenote/fix.md")).unwrap(), note);
        assert!(!p.root.join("CHANGELOG.md").exists());
        assert!(!p.root.join(".changenote/releases.json").exists());
    }
}

#[test]
fn templates_sections_code_and_saved_data_preserve_their_contracts() {
    let p = Project::new();
    p.configure(json!({"source_repository":REPOSITORY,"note_template":"{package}@{version} [{bump}]\n\n{summary}","sections":[{"title":"Fixes","types":["fix"]}]}));
    let summaries = [
        "```rs\n{package} $(touch forbidden)\n```",
        "Code:\n\n    indented code",
        "~~~\nunclosed fence",
        "Fix\n\n## 9.9.9\n\nDetails",
    ];
    for (i, summary) in summaries.iter().enumerate() {
        changenote::add_with_type(
            &p.root,
            &format!("note-{i}"),
            vec![("pkg".into(), Bump::Patch)],
            summary,
            Some("fix"),
        )
        .unwrap();
    }
    let sha = p.commit("notes and config");
    let details = status_with_note_sections(&p.root).unwrap();
    assert_eq!(details[0].note_sections[0].title, "Fixes");
    for (rendered, summary) in details[0].note_sections[0].notes.iter().zip(summaries) {
        assert_eq!(
            *rendered,
            format!(
                "pkg@1.0.1 [patch]\n\n{}",
                attributed(summary, std::slice::from_ref(&sha))
            )
        );
        assert_eq!(rendered.matches("/commit/").count(), 1);
    }
    let raw = changenote::status(&p.root).unwrap();
    assert_eq!(raw[0].notes, summaries);
    changenote::version(&p.root).unwrap();
    let body = release_notes(&p.root, "pkg", &Version::new(1, 0, 1)).unwrap();
    assert_eq!(body.matches("/commit/").count(), 4);
    assert!(body.contains("  {package} $(touch forbidden)"));
    assert!(!p.root.join("forbidden").exists());
    let saved: Value =
        serde_json::from_slice(&fs::read(p.root.join(".changenote/releases.json")).unwrap())
            .unwrap();
    assert_eq!(saved[0].as_object().unwrap().len(), 5);
}

#[test]
fn identical_summaries_shared_notes_and_group_only_members_keep_identity() {
    let p = Project::new();
    for name in ["a", "b", "c"] {
        fs::create_dir(p.root.join(name)).unwrap();
        fs::write(p.root.join(name).join("VERSION"), "1.0.0\n").unwrap();
    }
    fs::write(p.root.join(".changenote/config.json"), json!({"packages":{"a":{"path":"a","version_file":"VERSION"},"b":{"path":"b","version_file":"VERSION"},"c":{"path":"c","version_file":"VERSION"}},"fixed":[["a","b","c"]],"changelog":{"source_repository":REPOSITORY}}).to_string()).unwrap();
    changenote::add(
        &p.root,
        "shared",
        vec!["a".into(), "b".into()],
        Bump::Patch,
        "Same",
    )
    .unwrap();
    let shared = p.commit("shared note");
    changenote::add(&p.root, "separate", vec!["a".into()], Bump::Patch, "Same").unwrap();
    let separate = p.commit("separate note");
    let detail = status_with_note_sections(&p.root).unwrap();
    assert_eq!(
        detail[0].note_sections[0].notes,
        [
            attributed("Same", &[separate]),
            attributed("Same", std::slice::from_ref(&shared))
        ]
    );
    assert_eq!(
        detail[1].note_sections[0].notes,
        [attributed("Same", &[shared])]
    );
    assert!(detail[2].note_sections.is_empty());
    changenote::version(&p.root).unwrap();
    assert!(
        !fs::read_to_string(p.root.join("c/CHANGELOG.md"))
            .unwrap()
            .contains("/commit/")
    );
}

#[test]
fn nested_roots_and_literal_unusual_paths_do_not_select_sibling_history() {
    let mut p = Project::new();
    let outer = p.root.clone();
    p.root = outer.join("project with spaces");
    fs::create_dir(&p.root).unwrap();
    changenote::init(&p.root, "pkg", Version::new(1, 0, 0)).unwrap();
    p.configure(json!({"source_repository":REPOSITORY}));
    // Direct files can have names which the add command deliberately disallows.
    let name = ".changenote/中文 [*].md";
    fs::write(p.root.join(name), "---\npkg: patch\n---\n\nFix\n").unwrap();
    let sha = p.commit("nested note");
    fs::write(
        outer.join(".changenote/sibling.md"),
        "---\npkg: patch\n---\n\nSame\n",
    )
    .unwrap();
    p.git(&[
        "-C",
        outer.to_str().unwrap(),
        "add",
        "--",
        ".changenote/sibling.md",
    ]);
    p.git(&["-C", outer.to_str().unwrap(), "commit", "-qm", "sibling"]);
    assert_eq!(p.rendered(), attributed("Fix", &[sha]));
}

#[test]
fn disabled_output_still_renders_sources_but_never_writes_a_changelog() {
    let p = Project::new();
    p.configure(json!({"enabled":false,"source_repository":REPOSITORY}));
    let sha = p.note("fix", "Fix");
    assert_eq!(p.rendered(), attributed("Fix", &[sha]));
    changenote::version(&p.root).unwrap();
    assert!(!p.root.join("CHANGELOG.md").exists());
    assert!(release_notes(&p.root, "pkg", &Version::new(1, 0, 1)).is_err());
}

#[test]
fn malformed_repository_settings_fail_before_note_creation() {
    for value in [
        json!(null),
        json!(42),
        json!("http://github.com/a/b"),
        json!("https://user@github.com/a/b"),
        json!("https://github.com/a/b?x=1"),
        json!("https://github.com/a/b#x"),
        json!("https://gitlab.com/a/b"),
        json!("https://github.com/a/b/"),
        json!("https://github.com/a/b.git"),
        json!("https://github.com/a/.."),
        json!("https://github.com/a/b/extra"),
    ] {
        let p = Project::new();
        p.configure(json!({"source_repository":value}));
        assert!(
            changenote::add(&p.root, "fix", vec!["pkg".into()], Bump::Patch, "Fix").is_err(),
            "{value}"
        );
        assert!(!p.root.join(".changenote/fix.md").exists());
        assert!(changenote::version(&p.root).is_err());
        assert_eq!(
            fs::read_to_string(p.root.join("VERSION")).unwrap(),
            "1.0.0\n"
        );
    }
}

#[cfg(unix)]
#[test]
fn missing_git_and_non_utf8_names_preserve_default_and_opt_in_behavior() {
    use std::os::unix::ffi::OsStringExt;
    let p = Project::new();
    let filename = std::ffi::OsString::from_vec(b"invalid-\xff.md".to_vec());
    fs::write(
        p.root.join(".changenote").join(filename),
        "---\npkg: patch\n---\n\nFix\n",
    )
    .unwrap();
    let sha = p.commit("non UTF-8 note");
    assert_eq!(p.rendered(), attributed("Fix", &[sha]));
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(&p.root)
        .env("PATH", "/nonexistent")
        .args(["status", "--include-note-sections"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("/commit/"));
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(&p.root)
        .env("PATH", "/nonexistent")
        .arg("version")
        .output()
        .unwrap();
    assert!(!output.status.success());
    p.configure(json!({}));
    let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(&p.root)
        .env("PATH", "/nonexistent")
        .arg("version")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !fs::read_to_string(p.root.join("CHANGELOG.md"))
            .unwrap()
            .contains("Sources:")
    );
}

#[test]
fn unprovable_paused_notes_do_not_block_active_sources() {
    let p = Project::new();
    fs::create_dir(p.root.join("paused")).unwrap();
    fs::write(p.root.join("paused/VERSION"), "1.0.0\n").unwrap();
    let config = p.root.join(".changenote/config.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    value["packages"]["paused"] = json!({"path":"paused","version_file":"VERSION"});
    value["ignore"] = json!(["paused"]);
    fs::write(config, value.to_string()).unwrap();
    p.note("active", "Fix");
    changenote::add(
        &p.root,
        "paused",
        vec!["paused".into()],
        Bump::Patch,
        "Not committed",
    )
    .unwrap();
    changenote::version(&p.root).unwrap();
    assert!(p.root.join(".changenote/paused.md").exists());
}

#[test]
fn user_log_follow_does_not_merge_independent_same_content_notes() {
    let p = Project::new();
    let old = p.note("old", "Same");
    fs::copy(
        p.root.join(".changenote/old.md"),
        p.root.join(".changenote/new.md"),
    )
    .unwrap();
    let new = p.commit("copy as independent note");
    p.git(&["config", "log.follow", "true"]);
    let detail = status_with_note_sections(&p.root).unwrap();
    assert_eq!(
        detail[0].note_sections[0].notes,
        [attributed("Same", &[new]), attributed("Same", &[old])]
    );
    changenote::version(&p.root).unwrap();
}

#[test]
fn clean_crlf_clones_keep_sources_but_substantive_edits_still_fail() {
    let source = Project::new();
    let sha = source.note("fix", "Fix\nDetails");
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("crlf");
    source.git(&[
        "clone",
        "--quiet",
        "-c",
        "core.autocrlf=true",
        source.root.to_str().unwrap(),
        root.to_str().unwrap(),
    ]);
    let p = Project { _temp: temp, root };
    assert!(p.git(&["status", "--porcelain"]).is_empty());
    let path = p.root.join(".changenote/fix.md");
    assert!(
        fs::read(&path)
            .unwrap()
            .windows(2)
            .any(|pair| pair == b"\r\n")
    );
    assert_eq!(p.rendered(), attributed("Fix\nDetails", &[sha]));
    changenote::version(&p.root).unwrap();
    p.git(&["reset", "--hard", "HEAD"]);
    fs::write(&path, "---\r\npkg: patch\r\n---\r\n\r\nDifferent\r\n").unwrap();
    assert_eq!(p.rendered(), "Different");
    assert!(changenote::version(&p.root).is_err());
}

#[test]
fn partial_clones_do_not_fetch_missing_note_blobs() {
    let source = Project::new();
    source.note("fix", "Fix");
    source.git(&["config", "uploadpack.allowFilter", "true"]);
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("partial");
    source.git(&[
        "clone",
        "--quiet",
        "--filter=blob:none",
        "--no-checkout",
        &format!("file://{}", source.root.display()),
        root.to_str().unwrap(),
    ]);
    let p = Project { _temp: temp, root };
    fs::create_dir(p.root.join(".changenote")).unwrap();
    for path in ["VERSION", ".changenote/config.json", ".changenote/fix.md"] {
        fs::copy(source.root.join(path), p.root.join(path)).unwrap();
    }
    let missing = p.git(&["rev-list", "--objects", "--all", "--missing=print"]);
    assert!(missing.lines().any(|line| line.starts_with('?')));
    let trace = p.root.join(".git/source-trace");
    for command in ["status", "version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
            .current_dir(&p.root)
            .env("GIT_TRACE", &trace)
            .arg(command)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), command == "status");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("/commit/"));
    }
    assert_eq!(
        p.git(&["rev-list", "--objects", "--all", "--missing=print"]),
        missing
    );
    assert!(!fs::read_to_string(trace).unwrap().contains(" fetch "));
    assert_eq!(
        fs::read_to_string(p.root.join("VERSION")).unwrap(),
        "1.0.0\n"
    );
    assert!(p.root.join(".changenote/fix.md").exists());
}

#[test]
fn a_note_in_the_root_commit_has_a_provable_source() {
    let p = Project::new();
    changenote::add(
        &p.root,
        "root",
        vec!["pkg".into()],
        Bump::Patch,
        "Root change",
    )
    .unwrap();
    fs::remove_dir_all(p.root.join(".git")).unwrap();
    p.git(&["init", "-qb", "main"]);
    let source = p.commit("root commit with note");
    p.git(&["config", "log.showRoot", "false"]);
    assert_eq!(p.rendered(), attributed("Root change", &[source]));
    changenote::version(&p.root).unwrap();
}

#[cfg(unix)]
#[test]
fn a_file_type_change_starts_a_new_regular_note_history() {
    let p = Project::new();
    let path = p.root.join(".changenote/fix.md");
    std::os::unix::fs::symlink("unrelated", &path).unwrap();
    p.commit("old symlink");
    fs::remove_file(&path).unwrap();
    fs::write(&path, "---\npkg: patch\n---\n\nFix\n").unwrap();
    let source = p.commit("regular note replaces symlink");
    assert_eq!(p.rendered(), attributed("Fix", &[source]));
    changenote::version(&p.root).unwrap();
}
