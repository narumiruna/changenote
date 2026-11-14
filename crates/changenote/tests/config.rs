use changenote::{Bump, Config, load_config};
use semver::Version;
use serde_json::{Value, json};
use std::fs;

#[test]
fn init_includes_repository_schema() {
    let dir = tempfile::tempdir().unwrap();
    changenote::init(dir.path(), "pkg-a", Version::new(0, 0, 0)).unwrap();
    let config = load_config(dir.path()).unwrap();
    let schema: Value = serde_json::from_str(include_str!("../../../schema.json")).unwrap();
    assert_eq!(config.schema.as_deref().unwrap(), schema["$id"]);
}

#[test]
fn schema_metadata_is_optional_and_does_not_affect_releases() {
    for schema in [
        None,
        Some("./schema.json"),
        Some("https://example.invalid/schema.json"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        changenote::init(dir.path(), "pkg-a", Version::new(0, 0, 0)).unwrap();
        let mut config = json!({
            "packages": {"pkg-a": {"path": ".", "version_file": "VERSION"}}
        });
        if let Some(reference) = schema {
            config["$schema"] = json!(reference);
        }
        let path = dir.path().join(".changenote/config.json");
        let content = serde_json::to_string_pretty(&config).unwrap();
        fs::write(&path, &content).unwrap();
        let loaded = load_config(dir.path()).unwrap();
        assert_eq!(loaded.schema.as_deref(), schema);
        assert_eq!(serde_json::to_value(loaded).unwrap(), config);
        changenote::add(dir.path(), "fix", vec!["pkg-a".into()], Bump::Patch, "Fix").unwrap();
        assert_eq!(
            changenote::status(dir.path()).unwrap()[0].version,
            Version::new(0, 0, 1)
        );
        assert_eq!(
            changenote::version(dir.path()).unwrap()[0].version,
            Version::new(0, 0, 1)
        );
        assert_eq!(fs::read_to_string(path).unwrap(), content);
    }
}

#[test]
fn rejects_invalid_schema_types_and_unknown_fields() {
    let base = json!({
        "packages": {"pkg-a": {"path": ".", "version_file": "VERSION"}}
    });
    for value in [json!(42), json!(true), json!([]), json!({})] {
        let mut config = base.clone();
        config["$schema"] = value;
        assert!(serde_json::from_value::<Config>(config).is_err());
    }
    let mut config = base.clone();
    config["schema"] = json!("./schema.json");
    assert!(serde_json::from_value::<Config>(config).is_err());
    let mut config = base;
    config["packages"]["pkg-a"]["$schema"] = json!("./schema.json");
    assert!(serde_json::from_value::<Config>(config).is_err());
}
