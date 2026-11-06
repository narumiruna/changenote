//! Note parsing, validation, and creation.
use crate::Bump;
use crate::config;
use crate::config::ReleaseConfig;
use crate::paths;
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Note {
    pub packages: BTreeMap<String, Bump>,
    pub summary: String,
}

pub(crate) struct NoteData {
    note: Note,
    pub(crate) kind: Option<String>,
}

impl Deref for NoteData {
    type Target = Note;
    fn deref(&self) -> &Note {
        &self.note
    }
}

pub fn parse_note(content: &str) -> Result<Note> {
    Ok(parse_note_data(content)?.note)
}

fn parse_note_data(content: &str) -> Result<NoteData> {
    use anyhow::Context;

    let normalized = content.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    ensure!(lines.next() == Some("---"), "note must start with ---");
    let mut yaml = String::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line == "---" {
            closed = true;
            break;
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    ensure!(closed, "missing closing frontmatter delimiter");
    // Value rejects duplicate YAML keys before conversion to a map.
    let mut value: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
    let mapping = value
        .as_mapping_mut()
        .context("note frontmatter must be a mapping")?;
    let kind: Option<String> = mapping
        .remove(serde_yaml::Value::String("$type".into()))
        .map(serde_yaml::from_value)
        .transpose()?;
    if let Some(kind) = &kind {
        paths::safe_name(kind)?;
    }
    let packages: BTreeMap<String, Bump> = serde_yaml::from_value(value)?;
    ensure!(!packages.is_empty(), "note must name at least one package");
    let summary = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    ensure!(!summary.is_empty(), "note summary cannot be empty");
    Ok(NoteData {
        note: Note { packages, summary },
        kind,
    })
}

pub(crate) fn notes(root: &Path, config: &ReleaseConfig) -> Result<Vec<(PathBuf, NoteData)>> {
    use anyhow::Context;

    let directory = paths::checked_path(root, Path::new(".changenote"))?;
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut result = Vec::new();
    for path in paths {
        let name = path.file_name().unwrap().to_string_lossy();
        if path.extension().is_none_or(|e| e != "md")
            || name.starts_with('.')
            || ["README.md", "AGENTS.md", "CLAUDE.md", "GEMINI.md"]
                .iter()
                .any(|n| name.eq_ignore_ascii_case(n))
        {
            continue;
        }
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "note cannot be a symlink"
        );
        let note = parse_note_data(&fs::read_to_string(&path)?)
            .with_context(|| format!("invalid note {}", path.display()))?;
        if let (Some(changelog), Some(kind)) = (&config.changelog, &note.kind) {
            changelog.validate_type(kind)?;
        }
        for name in note.packages.keys() {
            ensure!(
                config.config.packages.contains_key(name),
                "unknown package {name} in {}",
                path.display()
            );
        }
        validate_note_pause(config, &note.packages)?;
        result.push((path, note));
    }
    Ok(result)
}

fn validate_note_pause(config: &ReleaseConfig, packages: &BTreeMap<String, Bump>) -> Result<()> {
    let paused = packages
        .keys()
        .filter(|name| config.ignore.contains(*name))
        .count();
    ensure!(
        paused == 0 || paused == packages.len(),
        "a note cannot mix ignored and active packages"
    );
    Ok(())
}

pub fn add(
    root: &Path,
    name: &str,
    packages: Vec<String>,
    bump: Bump,
    message: &str,
) -> Result<PathBuf> {
    add_with_bumps(
        root,
        name,
        packages.into_iter().map(|p| (p, bump)).collect(),
        message,
    )
}

/// Create one note with independently selected package bumps.
/// Conflicting assignments are rejected before any file is created.
pub fn add_with_bumps(
    root: &Path,
    name: &str,
    packages: Vec<(String, Bump)>,
    message: &str,
) -> Result<PathBuf> {
    add_with_type(root, name, packages, message, None)
}

/// Create a named note with independent package bumps and optional classification.
pub fn add_with_type(
    root: &Path,
    name: &str,
    packages: Vec<(String, Bump)>,
    message: &str,
    kind: Option<&str>,
) -> Result<PathBuf> {
    paths::safe_name(name)?;
    ensure!(
        !["README", "AGENTS", "CLAUDE", "GEMINI"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n)),
        "reserved note name"
    );
    let content = note_content(root, packages, message, kind)?;
    Ok(write_note(root, name, &content)?)
}

pub(crate) fn write_note(root: &Path, name: &str, content: &str) -> std::io::Result<PathBuf> {
    use std::io::Write;

    let path = root.join(".changenote").join(format!("{name}.md"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(content.as_bytes())?;
    Ok(path)
}

pub(crate) fn note_content(
    root: &Path,
    packages: Vec<(String, Bump)>,
    message: &str,
    kind: Option<&str>,
) -> Result<String> {
    ensure!(
        !packages.is_empty() && !message.trim().is_empty(),
        "packages and message are required"
    );
    let release_config = config::load_release_config(root)?;
    if let Some(kind) = kind {
        paths::safe_name(kind)?;
        if let Some(changelog) = &release_config.changelog {
            changelog.validate_type(kind)?;
        }
    }
    let config = &release_config.config;
    let mut changes = BTreeMap::new();
    for (pkg, bump) in packages {
        ensure!(config.packages.contains_key(&pkg), "unknown package {pkg}");
        if let Some(previous) = changes.insert(pkg.clone(), bump) {
            ensure!(previous == bump, "conflicting bumps for package {pkg}");
        }
    }
    validate_note_pause(&release_config, &changes)?;
    let mut metadata = serde_yaml::to_value(&changes)?;
    if let Some(kind) = kind {
        metadata.as_mapping_mut().unwrap().insert(
            serde_yaml::Value::String("$type".into()),
            serde_yaml::Value::String(kind.into()),
        );
    }
    Ok(format!(
        "---\n{}---\n\n{}\n",
        serde_yaml::to_string(&metadata)?,
        message.trim()
    ))
}
