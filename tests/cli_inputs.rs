use changenote::{init_with_version_file, load_config, parse_note};
use semver::Version;
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

fn input(root: &Path, args: &[&str], content: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(content).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn init_adopts_existing_sources_without_changing_bytes() {
    for (path, content) in [
        (
            "Cargo.toml",
            "# Keep this\n[package]\nname = 'pkg'\nversion = '1.2.3'\n",
        ),
        (
            "package.json",
            "{\r\n  \"version\": \"1.2.3\", \"other\": 1e2\r\n}",
        ),
        (
            "pyproject.toml",
            "[project]\nname = 'pkg'\nversion = '1.2.3'\n",
        ),
        ("VERSION", "  1.2.3\n"),
        ("packages/sdk/Cargo.toml", "[package]\nversion = '1.2.3'\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join(path);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, content).unwrap();
        let output = cli(
            dir.path(),
            &["init", "--name", "pkg", "--version-file", path],
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&source).unwrap(), content.as_bytes());
        let config = load_config(dir.path()).unwrap();
        let expected_parent = Path::new(path).parent().unwrap();
        let expected_parent = if expected_parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            expected_parent
        };
        assert_eq!(config.packages["pkg"].path, expected_parent);
        assert_eq!(
            config.packages["pkg"].version_file,
            Path::new(path).file_name().unwrap()
        );
        if path != "VERSION" {
            assert!(!dir.path().join("VERSION").exists());
        }
        let config_bytes = fs::read(dir.path().join(".changenote/config.json")).unwrap();
        assert!(
            !cli(
                dir.path(),
                &["init", "--name", "other", "--version-file", path]
            )
            .status
            .success()
        );
        assert_eq!(
            fs::read(dir.path().join(".changenote/config.json")).unwrap(),
            config_bytes
        );
    }
}

#[test]
fn invalid_init_sources_and_conflicting_options_do_not_create_configuration() {
    for (path, content) in [
        ("Cargo.toml", "[package]\nversion.workspace = true\n"),
        (
            "pyproject.toml",
            "[project]\nversion = '1.2.3'\ndynamic = ['version']\n",
        ),
        (
            "package.json",
            "{\"version\":\"1.0.0\",\"version\":\"2.0.0\"}",
        ),
        ("VERSION", "1.0.0-beta.1\n"),
        ("CHANGELOG.md", "1.2.3\n"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(path), content).unwrap();
        assert!(init_with_version_file(dir.path(), "pkg", Path::new(path)).is_err());
        assert!(!dir.path().join(".changenote").exists());
        assert_eq!(fs::read(dir.path().join(path)).unwrap(), content.as_bytes());
    }
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("VERSION"), "1.2.3\n").unwrap();
    for args in [
        vec![
            "init",
            "--name",
            "pkg",
            "--version-file",
            "VERSION",
            "--version",
            "1.2.3",
        ],
        vec!["init", "--name", "pkg", "--version-file", "../VERSION"],
        vec!["init", "--name", "pkg", "--version-file", "missing"],
        vec![
            "init",
            "--name",
            "invalid/name",
            "--version-file",
            "VERSION",
        ],
    ] {
        assert!(!cli(dir.path(), &args).status.success());
        assert!(!dir.path().join(".changenote").exists());
    }
    let absolute = dir.path().join("VERSION");
    assert!(init_with_version_file(dir.path(), "pkg", &absolute).is_err());
    assert!(!dir.path().join(".changenote").exists());
}

#[test]
fn default_init_and_explicit_initial_versions_remain_supported() {
    for version in [None, Some("2.3.4")] {
        let dir = tempfile::tempdir().unwrap();
        let mut args = vec!["init", "--name", "pkg"];
        if let Some(version) = version {
            args.extend(["--version", version]);
        }
        assert!(cli(dir.path(), &args).status.success());
        assert_eq!(
            fs::read_to_string(dir.path().join("VERSION")).unwrap(),
            format!("{}\n", version.unwrap_or("0.0.0"))
        );
    }
}

#[test]
fn file_and_stdin_messages_preserve_multiline_markdown() {
    let dir = tempfile::tempdir().unwrap();
    changenote::init(dir.path(), "pkg", Version::new(1, 0, 0)).unwrap();
    let message = "Migration instructions\n\n- Keep `quotes` and $variables\n\n```sh\n$(touch must-not-exist)\necho \"hello\"\n```\n";
    fs::write(dir.path().join("message.md"), message).unwrap();
    let out = cli(
        dir.path(),
        &[
            "add",
            "from-file",
            "--minor",
            "pkg",
            "--type",
            "feat",
            "--message-file",
            "message.md",
        ],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let note = fs::read_to_string(dir.path().join(".changenote/from-file.md")).unwrap();
    assert_eq!(parse_note(&note).unwrap().summary, message.trim());
    assert!(note.contains("$type: feat"));
    let out = input(
        dir.path(),
        &["add", "-p", "pkg", "--bump", "patch", "--message-file", "-"],
        message.as_bytes(),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let path = String::from_utf8(out.stdout).unwrap();
    assert!(path.contains("note-"));
    let note = fs::read_to_string(dir.path().join(path.trim())).unwrap();
    assert_eq!(parse_note(&note).unwrap().summary, message.trim());
    assert!(!dir.path().join("must-not-exist").exists());
}

#[test]
fn message_input_failures_do_not_allocate_notes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    changenote::init(root, "pkg", Version::new(1, 0, 0)).unwrap();
    changenote::add(
        root,
        "existing",
        vec!["pkg".into()],
        changenote::Bump::Patch,
        "Keep this pending note",
    )
    .unwrap();
    let notes = || {
        fs::read_dir(root.join(".changenote"))
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect::<BTreeMap<_, _>>()
    };
    let before = notes();
    for name in [Some("empty-message"), None] {
        let mut args = vec!["add"];
        if let Some(name) = name {
            args.push(name);
        }
        args.extend(["--patch", "pkg"]);
        for message in ["", " \t\n", " \t\r\n\u{00a0}\u{2003}\r\n"] {
            fs::write(root.join("empty"), message).unwrap();
            for (option, value) in [
                ("-m", message),
                ("--message-file", "empty"),
                ("--message-file", "-"),
            ] {
                let mut command = args.clone();
                command.extend([option, value]);
                let out = if option == "--message-file" && value == "-" {
                    input(root, &command, message.as_bytes())
                } else {
                    cli(root, &command)
                };
                assert_eq!(
                    out.status.code(),
                    Some(1),
                    "accepted {command:?}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
                assert!(out.stdout.is_empty());
                assert_eq!(notes(), before, "changed notes for {command:?}");
            }
        }
    }
    fs::write(root.join("bad-utf8"), [0xff, 0xfe]).unwrap();
    fs::create_dir(root.join("directory")).unwrap();
    for name in ["bad-utf8", "missing", "directory"] {
        assert!(
            !cli(root, &["add", "--patch", "pkg", "--message-file", name])
                .status
                .success()
        );
    }
    assert!(
        !input(
            root,
            &["add", "--patch", "pkg", "--message-file", "-"],
            &[0xff, 0xfe]
        )
        .status
        .success()
    );
    for args in [
        vec!["add", "--patch", "pkg"],
        vec![
            "add",
            "--patch",
            "pkg",
            "-m",
            "Inline",
            "--message-file",
            "empty",
        ],
    ] {
        assert_eq!(cli(root, &args).status.code(), Some(2));
    }
    assert_eq!(notes(), before);
}

#[test]
fn init_rejects_invalid_existing_changelog_without_partial_configuration() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("VERSION"), "1.2.3\n").unwrap();
    fs::create_dir(dir.path().join("CHANGELOG.md")).unwrap();
    assert!(init_with_version_file(dir.path(), "pkg", Path::new("VERSION")).is_err());
    assert!(!dir.path().join(".changenote").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("VERSION")).unwrap(),
        "1.2.3\n"
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_in_repository_source_directory_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("real")).unwrap();
    fs::write(dir.path().join("real/VERSION"), "1.2.3\n").unwrap();
    std::os::unix::fs::symlink("real", dir.path().join("linked")).unwrap();
    assert!(init_with_version_file(dir.path(), "pkg", Path::new("linked/VERSION")).is_err());
    assert!(!dir.path().join(".changenote").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("real/VERSION")).unwrap(),
        "1.2.3\n"
    );
}

#[cfg(unix)]
#[test]
fn init_rejects_source_and_configuration_symlinks() {
    use std::os::unix::fs::symlink;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("actual"), "1.2.3\n").unwrap();
    symlink("actual", dir.path().join("VERSION")).unwrap();
    assert!(init_with_version_file(dir.path(), "pkg", Path::new("VERSION")).is_err());
    assert!(!dir.path().join(".changenote").exists());
    fs::remove_file(dir.path().join("VERSION")).unwrap();
    symlink("missing", dir.path().join("VERSION")).unwrap();
    assert!(changenote::init(dir.path(), "pkg", Version::new(1, 0, 0)).is_err());
    fs::remove_file(dir.path().join("VERSION")).unwrap();
    symlink("missing-directory", dir.path().join(".changenote")).unwrap();
    assert!(init_with_version_file(dir.path(), "pkg", Path::new("actual")).is_err());
    assert!(!dir.path().join("missing-directory").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("actual")).unwrap(),
        "1.2.3\n"
    );
}
