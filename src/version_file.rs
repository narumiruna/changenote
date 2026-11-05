use crate::json_value::{self, JsonValue};
use anyhow::{Context, Result, ensure};
use semver::Version;
use std::fs;
use std::ops::Range;
use std::path::Path;
use toml_edit::DocumentMut;

// Manifest filenames and the xcconfig extension select fields. Other files remain plain text.
enum VersionFile {
    Plain,
    Json {
        document: JsonValue,
        content: String,
        span: Range<usize>,
    },
    Toml(DocumentMut, &'static str),
    Xcconfig {
        content: String,
        span: Range<usize>,
    },
}

impl VersionFile {
    fn load(path: &Path) -> Result<(Self, Version)> {
        let content = fs::read_to_string(path)?;
        let file = match path.file_name().and_then(|name| name.to_str()) {
            Some("package.json") => Self::Json {
                document: serde_json::from_str(&content)?,
                span: json_value::version_span(&content)?,
                content: content.clone(),
            },
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
            _ if path.extension().is_some_and(|ext| ext == "xcconfig") => Self::Xcconfig {
                span: xcconfig_version_span(&content)?,
                content: content.clone(),
            },
            _ => Self::Plain,
        };
        let text =
            match &file {
                Self::Plain => content.trim().to_owned(),
                Self::Xcconfig { content, span } => content[span.clone()].to_owned(),
                Self::Json { document, .. } => document
                    .get("version")
                    .and_then(JsonValue::as_string)
                    .context("package.json requires a string version field")?,
                Self::Toml(document, section) => document
                    .get(section)
                    .and_then(|item| item.get("version"))
                    .and_then(|item| item.as_str())
                    .with_context(|| format!("{section}.version must be an explicit string"))?
                    .to_owned(),
            };
        let version = Version::parse(&text)?;
        ensure!(
            version.pre.is_empty() && version.build.is_empty(),
            "only stable SemVer versions are supported"
        );
        Ok((file, version))
    }
}

fn xcconfig_version_span(content: &str) -> Result<Range<usize>> {
    ensure!(
        !content.contains('\u{feff}'),
        "xcconfig version files do not support a byte order mark (BOM)"
    );
    ensure!(
        content
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')
                || (!c.is_control() && !c.is_whitespace())),
        "xcconfig version files support only spaces, tabs, LF, and CRLF whitespace"
    );
    let mut span = None;
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        ensure!(
            !line.strip_suffix("\r\n").unwrap_or(line).contains('\r'),
            "xcconfig version files require LF or CRLF line endings"
        );
        let code = line.split_once("//").map_or(line, |(code, _)| code);
        // A trailing backslash before a // comment also continues the setting.
        ensure!(
            [line, code].into_iter().all(|part| !part
                .trim_end_matches([' ', '\t', '\r', '\n'])
                .ends_with('\\')),
            "xcconfig version files do not support line continuations"
        );
        ensure!(
            !code.contains("/*") && !code.contains("*/"),
            "xcconfig version files support only // comments"
        );
        let setting = code.trim_matches([' ', '\t', '\r', '\n']);
        ensure!(
            !setting.starts_with('#'),
            "xcconfig version files do not support include or other directives"
        );
        let Some(rest) = setting.strip_prefix("MARKETING_VERSION") else {
            continue;
        };
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let value = rest
            .trim_start_matches([' ', '\t'])
            .strip_prefix('=')
            .context("xcconfig requires an unconditional MARKETING_VERSION = <version> assignment")?
            .trim_matches([' ', '\t']);
        ensure!(
            span.is_none(),
            "xcconfig requires exactly one MARKETING_VERSION assignment"
        );
        let end = start + code.trim_end_matches([' ', '\t', '\r', '\n']).len();
        span = Some(end - value.len()..end);
    }
    span.context("xcconfig requires exactly one MARKETING_VERSION assignment")
}

pub(crate) fn read_version(path: &Path) -> Result<Version> {
    VersionFile::load(path)
        .map(|(_, version)| version)
        .with_context(|| format!("invalid version in {}", path.display()))
}

pub(crate) fn version_content(path: &Path, version: &Version, legacy: bool) -> Result<String> {
    let (file, _) = VersionFile::load(path)
        .with_context(|| format!("invalid version in {}", path.display()))?;
    match file {
        VersionFile::Plain => Ok(format!("{version}\n")),
        VersionFile::Xcconfig { mut content, span } => {
            content.replace_range(span, &version.to_string());
            Ok(content)
        }
        VersionFile::Json {
            mut document,
            mut content,
            span,
        } => {
            let value = serde_json::to_string(&version.to_string())?;
            if legacy {
                *document.get_mut("version").context("missing version")? =
                    serde_json::from_str(&value)?;
                Ok(format!("{}\n", serde_json::to_string_pretty(&document)?))
            } else {
                content.replace_range(span, &value);
                Ok(content)
            }
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
