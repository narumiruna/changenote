use changenote::{Bump, add_with_bumps, status, version};
use std::fs;

#[test]
fn mixed_example_uses_each_packages_own_note_bump() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join(".changenote")).unwrap();
    for name in ["pkg-python", "pkg-node"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(dir.path().join(name).join("VERSION"), "1.2.3\n").unwrap();
    }
    fs::write(dir.path().join(".changenote/config.json"), r#"{"packages":{"pkg-python":{"path":"pkg-python","version_file":"VERSION"},"pkg-node":{"path":"pkg-node","version_file":"VERSION"}}}"#).unwrap();
    add_with_bumps(
        dir.path(),
        "shared",
        vec![
            ("pkg-python".into(), Bump::Minor),
            ("pkg-node".into(), Bump::Patch),
        ],
        "Update shared API",
    )
    .unwrap();
    version(dir.path()).unwrap();
    for (name, version, heading) in [
        ("pkg-python", "1.3.0", "Minor"),
        ("pkg-node", "1.2.4", "Patch"),
    ] {
        assert_eq!(
            fs::read_to_string(dir.path().join(name).join("CHANGELOG.md")).unwrap(),
            format!("## {version}\n\n### {heading} Changes\n\n- Update shared API\n\n")
        );
    }
}

// These summaries, starting versions, and expected entries match the isolated
// examples in changenote-example. Release mutations remain in Rust temp fixtures.
#[test]
fn example_changelogs_have_exact_grouping_and_markdown() {
    for (scenario, notes, expected) in [
        (
            "patch",
            vec![("a", Bump::Patch, "Fix greeting")],
            "## 1.2.4\n\n### Patch Changes\n\n- Fix greeting\n\n",
        ),
        (
            "highest",
            vec![
                ("a", Bump::Patch, "Fix greeting"),
                ("b", Bump::Minor, "Add greeting option"),
                ("c", Bump::Patch, "Fix output"),
            ],
            "## 1.3.0\n\n### Minor Changes\n\n- Add greeting option\n\n### Patch Changes\n\n- Fix greeting\n\n- Fix output\n\n",
        ),
        (
            "multiline",
            vec![(
                "a",
                Bump::Minor,
                "Add greeting option\n\n- Keep existing defaults.\n\n```sh\nhello --name Ada\n```",
            )],
            "## 1.3.0\n\n### Minor Changes\n\n- Add greeting option\n  \n  - Keep existing defaults.\n  \n  ```sh\n  hello --name Ada\n  ```\n\n",
        ),
        (
            "all-groups",
            vec![
                ("a", Bump::Patch, "Fix"),
                ("b", Bump::Minor, "Feature"),
                ("c", Bump::Major, "Migration"),
            ],
            "## 2.0.0\n\n### Major Changes\n\n- Migration\n\n### Minor Changes\n\n- Feature\n\n### Patch Changes\n\n- Fix\n\n",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        changenote::init(dir.path(), "pkg-python", "1.2.3".parse().unwrap()).unwrap();
        for (name, bump, summary) in notes {
            add_with_bumps(dir.path(), name, vec![("pkg-python".into(), bump)], summary).unwrap();
        }
        let before = status(dir.path()).unwrap();
        assert_eq!(version(dir.path()).unwrap()[0].version, before[0].version);
        assert_eq!(
            fs::read_to_string(dir.path().join("CHANGELOG.md")).unwrap(),
            expected,
            "{scenario}"
        );
        let batch = fs::read(dir.path().join(".changenote/releases.json")).unwrap();
        assert!(version(dir.path()).unwrap().is_empty());
        assert_eq!(
            fs::read(dir.path().join(".changenote/releases.json")).unwrap(),
            batch
        );
    }
}
