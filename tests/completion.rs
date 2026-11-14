use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn cli(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_changenote"))
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn completion_scripts_include_commands_options_and_values_without_configuration() {
    for (shell, registration, root_option) in [
        ("bash", "complete -F _changenote", "--root"),
        ("zsh", "#compdef changenote", "--root"),
        ("fish", "complete -c changenote", "-l root"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let output = cli(dir.path(), &["completion", shell]);
        assert!(
            output.status.success(),
            "{shell}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let script = String::from_utf8(output.stdout).unwrap();
        for token in [
            registration,
            root_option,
            "init",
            "add",
            "status",
            "check",
            "doctor",
            "version",
            "release-notes",
            "tag",
            "completion",
            "include-note-paths",
            "message-file",
            "coverage",
            "major",
            "minor",
            "patch",
        ] {
            assert!(script.contains(token), "{shell} script lacks {token:?}");
        }
        // clap_complete does not generate positional argument values for Fish.
        if shell != "fish" {
            for value in ["bash", "zsh", "fish"] {
                assert!(script.contains(value), "{shell} script lacks {value:?}");
            }
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn completion_ignores_root_configuration_and_does_not_change_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".changenote/config.json");
    fs::create_dir(config.parent().unwrap()).unwrap();
    fs::write(&config, "invalid configuration").unwrap();
    fs::write(dir.path().join("VERSION"), "1.2.3\n").unwrap();

    for shell in ["bash", "zsh", "fish"] {
        let output = cli(dir.path(), &["completion", shell]);
        let missing = cli(dir.path(), &["--root", "missing", "completion", shell]);
        assert!(output.status.success());
        assert!(missing.status.success());
        assert!(output.stderr.is_empty());
        assert!(missing.stderr.is_empty());
        assert_eq!(output.stdout, missing.stdout);
    }

    assert_eq!(fs::read_to_string(config).unwrap(), "invalid configuration");
    assert_eq!(
        fs::read_to_string(dir.path().join("VERSION")).unwrap(),
        "1.2.3\n"
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    assert_eq!(
        fs::read_dir(dir.path().join(".changenote"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn completion_rejects_missing_unsupported_and_extra_arguments() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["completion"],
        vec!["completion", "nu"],
        vec!["completion", "bash", "extra"],
    ] {
        let output = cli(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn completion_help_lists_the_supported_shells() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(dir.path(), &["completion", "--help"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("Usage: changenote completion"));
    assert!(help.contains("<SHELL>"));
    assert!(help.contains("[possible values: bash, zsh, fish]"));
}
