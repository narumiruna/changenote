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
fn skill_prints_the_exact_bundled_markdown_without_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let output = cli(dir.path(), &["skill"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        include_bytes!("../assets/changenote.SKILL.md").as_slice()
    );
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn skill_ignores_root_configuration_and_local_skill_files() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join(".changenote/config.json");
    let local_skill = dir.path().join("skills/changenote/SKILL.md");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::create_dir_all(local_skill.parent().unwrap()).unwrap();
    fs::write(&config, "invalid configuration").unwrap();
    fs::write(&local_skill, "local skill must not be read\n").unwrap();

    for args in [
        vec!["skill"],
        vec!["--root", "missing", "skill"],
        vec!["skill", "--root", "missing"],
    ] {
        let output = cli(dir.path(), &args);
        assert!(output.status.success(), "{args:?}");
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout,
            include_bytes!("../assets/changenote.SKILL.md").as_slice()
        );
    }

    assert_eq!(fs::read_to_string(config).unwrap(), "invalid configuration");
    assert_eq!(
        fs::read_to_string(local_skill).unwrap(),
        "local skill must not be read\n"
    );
    assert!(!dir.path().join("missing").exists());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn skill_help_is_available_without_configuration() {
    let dir = tempfile::tempdir().unwrap();
    for args in [vec!["--help"], vec!["skill", "--help"]] {
        let output = cli(dir.path(), &args);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let help = String::from_utf8(output.stdout).unwrap();
        assert!(help.contains("skill"));
        assert!(help.contains("Print the bundled agent skill"));
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn skill_rejects_extra_arguments_and_unknown_options() {
    let dir = tempfile::tempdir().unwrap();
    for args in [vec!["skill", "extra"], vec!["skill", "--unknown"]] {
        let output = cli(dir.path(), &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
