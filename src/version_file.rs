use anyhow::{Context, Result, ensure};
use semver::Version;
use std::{fs, path::Path};
use toml_edit::DocumentMut;

// The configured filename selects the version field. Other filenames remain plain text.
enum VersionFile {
    Plain,
    Json(serde_json::Value),
    Toml(DocumentMut, &'static str),
}

impl VersionFile {
    fn load(path: &Path) -> Result<(Self, Version)> {
        let content = fs::read_to_string(path)?;
        let file = match path.file_name().and_then(|name| name.to_str()) {
            Some("package.json") => Self::Json(serde_json::from_str(&content)?),
            Some(name @ ("Cargo.toml" | "pyproject.toml")) => {
                let document = content.parse::<DocumentMut>()?;
                let section = if name == "Cargo.toml" {
                    "package"
                } else {
                    "project"
                };
                if section == "project" {
                    ensure!(
                        !document
                            .get(section)
                            .and_then(|item| item.get("dynamic"))
                            .and_then(|item| item.as_array())
                            .is_some_and(|values| values
                                .iter()
                                .any(|value| value.as_str() == Some("version"))),
                        "dynamic project.version is not supported"
                    );
                }
                Self::Toml(document, section)
            }
            _ => Self::Plain,
        };
        let text = match &file {
            Self::Plain => content.trim(),
            Self::Json(document) => document
                .get("version")
                .and_then(|value| value.as_str())
                .context("package.json requires a string version field")?,
            Self::Toml(document, section) => document
                .get(section)
                .and_then(|item| item.get("version"))
                .and_then(|item| item.as_str())
                .with_context(|| format!("{section}.version must be an explicit string"))?,
        };
        let version = Version::parse(text)?;
        ensure!(
            version.pre.is_empty() && version.build.is_empty(),
            "only stable SemVer versions are supported"
        );
        Ok((file, version))
    }
}

pub(crate) fn read_version(path: &Path) -> Result<Version> {
    VersionFile::load(path)
        .map(|(_, version)| version)
        .with_context(|| format!("invalid version in {}", path.display()))
}

pub(crate) fn version_content(path: &Path, version: &Version) -> Result<String> {
    let (file, _) = VersionFile::load(path)
        .with_context(|| format!("invalid version in {}", path.display()))?;
    match file {
        VersionFile::Plain => Ok(format!("{version}\n")),
        VersionFile::Json(mut document) => {
            document["version"] = serde_json::Value::String(version.to_string());
            Ok(format!("{}\n", serde_json::to_string_pretty(&document)?))
        }
        VersionFile::Toml(mut document, section) => {
            let value = document[section]["version"]
                .as_value_mut()
                .context("missing version")?;
            let decor = value.decor().clone();
            *value = toml_edit::Value::from(version.to_string());
            *value.decor_mut() = decor;
            Ok(document.to_string())
        }
    }
}
