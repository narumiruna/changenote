use serde_json::Value;
use std::{fs, path::Path, process::Command};

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(path, target).unwrap();
        }
    }
}

#[test]
fn exact_planning_examples_match_status_and_generated_changelogs() {
    for name in ["provenance", "fixed-equal", "fixed-unequal"] {
        let example = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/planning")
            .join(name);
        let dir = tempfile::tempdir().unwrap();
        copy_tree(&example.join("fixture"), dir.path());
        for (args, expected) in [
            (vec!["status"], "expected-status.json"),
            (
                vec!["status", "--include-note-paths", "--include-note-bumps"],
                "expected-details.json",
            ),
        ] {
            let output = Command::new(env!("CARGO_BIN_EXE_changenote"))
                .current_dir(dir.path())
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
            let expected: Value =
                serde_json::from_slice(&fs::read(example.join(expected)).unwrap()).unwrap();
            assert_eq!(actual, expected, "{name}");
        }
        changenote::version(dir.path()).unwrap();
        for package in ["pkg-node", "pkg-python"] {
            assert_eq!(
                fs::read(
                    dir.path()
                        .join("packages")
                        .join(package)
                        .join("CHANGELOG.md")
                )
                .unwrap(),
                fs::read(example.join(format!("expected-{package}-CHANGELOG.md"))).unwrap(),
                "{name}/{package}"
            );
        }
    }
}
