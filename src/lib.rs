mod changelog;
mod sections;
pub use sections::NoteSection;
mod generated;
mod patterns;
mod planning;
pub use generated::{add_generated, add_generated_with_type};
mod check;
pub use planning::{
    DetailedRelease, FixedGroup, SectionedRelease, status_with_note_paths,
    status_with_note_sections,
};
mod json_value;
pub use check::{
    CheckReport, Coverage, CoverageReport, check, check_with_coverage, configured_base_branch,
};
mod doctor;
pub use doctor::{DoctorCheck, DoctorReport, DoctorStatus, doctor};
mod lockfile;
mod transaction;
mod version_file;

use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
};
use version_file::{read_version, version_content};

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub path: PathBuf,
    pub version_file: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub packages: BTreeMap<String, Package>,
}

#[derive(Debug)]
pub struct Note {
    pub packages: BTreeMap<String, Bump>,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Release {
    pub package: String,
    pub old_version: Version,
    pub version: Version,
    pub bump: Bump,
    pub tag: String,
    pub notes: Vec<String>,
    /// Bump for each summary, in the same order as `notes`.
    #[serde(skip)]
    pub note_bumps: Vec<Bump>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SavedRelease {
    package: String,
    old_version: Version,
    version: Version,
    bump: Bump,
    tag: String,
}

impl From<&Release> for SavedRelease {
    fn from(release: &Release) -> Self {
        Self {
            package: release.package.clone(),
            old_version: release.old_version.clone(),
            version: release.version.clone(),
            bump: release.bump,
            tag: release.tag.clone(),
        }
    }
}

/// Read one package's exact changelog version body without changing files.
pub fn release_notes(root: &Path, package: &str, version: &Version) -> Result<String> {
    let config = load_config(root)?;
    let pkg = config
        .packages
        .get(package)
        .context("unknown release package")?;
    let path = checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "changelog cannot be a symlink"
    );
    changelog::extract(&fs::read_to_string(path)?, version)
}

fn safe_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
        "name must contain only ASCII letters, digits, hyphens, or underscores: {name}"
    );
    Ok(())
}

fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "path must be relative without parent components: {}",
        path.display()
    );
    Ok(())
}

// Resolve existing paths before writes so symlinks cannot escape the repository.
fn checked_path(root: &Path, path: &Path) -> Result<PathBuf> {
    relative(path)?;
    let root = root.canonicalize()?;
    let full = root.join(path);
    let existing = match fs::symlink_metadata(&full) {
        Ok(_) => full.as_path(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            full.parent().context("missing parent")?
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        existing.canonicalize()?.starts_with(&root),
        "path escapes repository: {}",
        path.display()
    );
    Ok(full)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(rename = "$schema")]
    schema: Option<String>,
    packages: BTreeMap<String, PackageFile>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    base_branch: Option<String>,
    #[serde(default)]
    changed_file_patterns: patterns::FilePatterns,
    #[serde(default)]
    fixed: Vec<Vec<String>>,
    changelog: Option<sections::ChangelogConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFile {
    path: PathBuf,
    version_file: PathBuf,
    #[serde(default)]
    version_outputs: Vec<PathBuf>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    tag_prefix: Option<String>,
}

fn deserialize_optional_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

struct PackageOptions {
    version_outputs: Vec<PathBuf>,
    tag_prefix: Option<String>,
}

pub(crate) struct ReleaseConfig {
    config: Config,
    base_branch: Option<String>,
    changed_file_patterns: patterns::FilePatterns,
    fixed: Vec<Vec<String>>,
    changelog: Option<sections::ChangelogConfig>,
    package_options: BTreeMap<String, PackageOptions>,
}

struct NoteData {
    note: Note,
    kind: Option<String>,
}

impl std::ops::Deref for NoteData {
    type Target = Note;
    fn deref(&self) -> &Note {
        &self.note
    }
}

/// Validate all configuration and return only the primary package settings.
/// Extended settings, including the check base ref, are not retained in this view.
pub fn load_config(root: &Path) -> Result<Config> {
    Ok(load_release_config(root)?.config)
}

fn load_release_config(root: &Path) -> Result<ReleaseConfig> {
    let path = checked_path(root, Path::new(".changenote/config.json"))?;
    let file: ConfigFile = serde_json::from_str(&fs::read_to_string(path)?)?;
    if let Some(changelog) = &file.changelog {
        changelog.validate()?;
    }
    if let Some(reference) = &file.base_branch {
        ensure!(
            !reference.is_empty()
                && reference.trim() == reference
                && !reference.chars().any(char::is_control),
            "base_branch must be nonempty, trimmed, and contain no control characters"
        );
    }
    let mut packages = BTreeMap::new();
    let mut package_options = BTreeMap::new();
    for (name, package) in file.packages {
        packages.insert(
            name.clone(),
            Package {
                path: package.path,
                version_file: package.version_file,
            },
        );
        package_options.insert(
            name,
            PackageOptions {
                version_outputs: package.version_outputs,
                tag_prefix: package.tag_prefix,
            },
        );
    }
    let config = Config {
        schema: file.schema,
        packages,
    };
    let mut members = std::collections::BTreeSet::new();
    for group in &file.fixed {
        ensure!(
            group.len() >= 2,
            "fixed groups require at least two packages"
        );
        for name in group {
            ensure!(
                config.packages.contains_key(name),
                "unknown fixed package {name}"
            );
            ensure!(
                members.insert(name),
                "duplicate or overlapping fixed package {name}"
            );
        }
    }
    ensure!(
        !config.packages.is_empty(),
        "config must contain at least one package"
    );
    let mut files = std::collections::BTreeSet::new();
    for (name, pkg) in &config.packages {
        safe_name(name)?;
        relative(&pkg.path)?;
        relative(&pkg.version_file)?;
        let directory = checked_path(root, &pkg.path)?;
        ensure!(
            directory.is_dir(),
            "package directory does not exist: {}",
            directory.display()
        );
        let file = checked_path(root, &pkg.path.join(&pkg.version_file))?;
        ensure!(
            file.is_file(),
            "version file does not exist: {}",
            file.display()
        );
        ensure!(
            !file
                .canonicalize()?
                .starts_with(root.canonicalize()?.join(".changenote")),
            "version files cannot be in .changenote"
        );
        ensure!(
            files.insert(file.canonicalize()?),
            "packages cannot share a version file"
        );
        let changelog = checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
        let changelog_target = if changelog.exists() {
            ensure!(changelog.is_file(), "changelog must be a file");
            changelog.canonicalize()?
        } else {
            changelog
                .parent()
                .context("missing parent")?
                .canonicalize()?
                .join("CHANGELOG.md")
        };
        ensure!(
            files.insert(changelog_target),
            "package output paths must not overlap"
        );
        let version = read_version(&file)?;
        let options = &package_options[name];
        if let Some(prefix) = &options.tag_prefix {
            validate_tag(&format!("{prefix}{version}"))?;
        }
        for output in &options.version_outputs {
            relative(output)?;
            let output = checked_path(root, &pkg.path.join(output))?;
            reject_output_symlinks(root, &output)?;
            ensure!(
                output.is_file(),
                "version output must be a file: {}",
                output.display()
            );
            let target = output.canonicalize()?;
            ensure!(
                !target.starts_with(root.canonicalize()?.join(".changenote")),
                "version outputs cannot be in .changenote"
            );
            ensure!(
                files.insert(target),
                "package output paths must not overlap"
            );
            ensure!(
                read_version(&output)? == version,
                "version output does not match primary version: {}",
                output.display()
            );
        }
    }
    Ok(ReleaseConfig {
        config,
        base_branch: file.base_branch,
        changed_file_patterns: file.changed_file_patterns,
        fixed: file.fixed,
        changelog: file.changelog,
        package_options,
    })
}

fn reject_output_symlinks(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let mut current = root.clone();
    for component in path.strip_prefix(&root)?.components() {
        current.push(component);
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "version output cannot contain a symlink: {}",
            current.display()
        );
    }
    Ok(())
}

/// Return checked absolute paths for every primary and additional release version file.
pub(crate) fn release_version_paths(
    root: &Path,
    config: &ReleaseConfig,
    releases: &[Release],
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for release in releases {
        let package = config
            .config
            .packages
            .get(&release.package)
            .context("unknown release package")?;
        for path in std::iter::once(&package.version_file)
            .chain(&config.package_options[&release.package].version_outputs)
        {
            paths.push(checked_path(root, &package.path.join(path))?);
        }
    }
    Ok(paths)
}

// Match Git's full ref-name rules without requiring a Git executable for planning.
fn validate_tag(tag: &str) -> Result<()> {
    ensure!(
        !tag.is_empty()
            && !tag.starts_with('-')
            && !tag.ends_with('.')
            && !tag.contains("..")
            && !tag.contains("@{")
            && !tag
                .bytes()
                .any(|b| b <= b' ' || b == 127 || b"~^:?*[\\\\".contains(&b))
            && tag
                .split('/')
                .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock")),
        "invalid release tag: {tag}"
    );
    Ok(())
}

fn tag_namespace_conflict(a: &str, b: &str) -> bool {
    a.strip_prefix(b)
        .is_some_and(|suffix| suffix.starts_with('/'))
        || b.strip_prefix(a)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn reserve_release_tag(tags: &mut std::collections::BTreeSet<String>, tag: &str) -> Result<()> {
    ensure!(!tags.contains(tag), "duplicate release tag: {tag}");
    for other in tags.iter() {
        ensure!(
            !tag_namespace_conflict(tag, other),
            "release tag namespace conflict: {tag} and {other}"
        );
    }
    tags.insert(tag.to_owned());
    Ok(())
}

fn release_tag(config: &ReleaseConfig, package: &str, version: &Version) -> Result<String> {
    let tag = if let Some(prefix) = &config.package_options[package].tag_prefix {
        format!("{prefix}{version}")
    } else if config.config.packages.len() == 1 {
        format!("v{version}")
    } else {
        format!("changenote/{package}@v{version}")
    };
    validate_tag(&tag)?;
    Ok(tag)
}

pub fn parse_note(content: &str) -> Result<Note> {
    Ok(parse_note_data(content)?.note)
}

fn parse_note_data(content: &str) -> Result<NoteData> {
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
        safe_name(kind)?;
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

fn notes(root: &Path, config: &ReleaseConfig) -> Result<Vec<(PathBuf, NoteData)>> {
    let directory = checked_path(root, Path::new(".changenote"))?;
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
        result.push((path, note));
    }
    Ok(result)
}

fn plan(
    root: &Path,
    config: &ReleaseConfig,
    notes: &[(PathBuf, NoteData)],
) -> Result<Vec<Release>> {
    Ok(planning::plan(root, config, notes)?
        .into_iter()
        .map(|r| r.detail.release)
        .collect())
}

pub fn status(root: &Path) -> Result<Vec<Release>> {
    let config = load_release_config(root)?;
    plan(root, &config, &notes(root, &config)?)
}

pub fn init(root: &Path, name: &str, version: Version) -> Result<()> {
    safe_name(name)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "only stable SemVer versions are supported"
    );
    ensure!(
        !root.join("VERSION").try_exists()? && !root.join("VERSION").is_symlink(),
        "VERSION already exists; use init --version-file VERSION"
    );
    create_initial_config(root, name, Path::new("VERSION"), Some(version))
}

/// Adopt an existing version file without changing its bytes.
/// The source directory becomes the configured package directory.
pub fn init_with_version_file(root: &Path, name: &str, version_file: &Path) -> Result<()> {
    safe_name(name)?;
    let source = checked_path(root, version_file)?;
    reject_output_symlinks(root, &source)?;
    ensure!(
        !fs::symlink_metadata(&source)?.file_type().is_symlink() && source.is_file(),
        "version file must be a regular file, not a symlink"
    );
    ensure!(
        !source
            .canonicalize()?
            .starts_with(root.canonicalize()?.join(".changenote")),
        "version files cannot be in .changenote"
    );
    ensure!(
        version_file
            .file_name()
            .is_some_and(|name| name != "CHANGELOG.md"),
        "version file cannot overlap the changelog"
    );
    read_version(&source)?;
    let directory = version_file.parent().context("missing version directory")?;
    let changelog = checked_path(root, &directory.join("CHANGELOG.md"))?;
    match fs::symlink_metadata(&changelog) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "changelog must be a regular file, not a symlink"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    create_initial_config(root, name, version_file, None)
}

fn create_initial_config(
    root: &Path,
    name: &str,
    version_file: &Path,
    initial_version: Option<Version>,
) -> Result<()> {
    let directory = version_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let config = Config {
        schema: Some(
            "https://raw.githubusercontent.com/narumiruna/changenote/main/schema.json".to_owned(),
        ),
        packages: BTreeMap::from([(
            name.to_owned(),
            Package {
                path: directory.to_owned(),
                version_file: version_file
                    .file_name()
                    .context("missing version filename")?
                    .into(),
            },
        )]),
    };
    let content = format!("{}\n", serde_json::to_string_pretty(&config)?);
    let notes_directory = root.join(".changenote");
    fs::create_dir(&notes_directory)
        .context("cannot create .changenote; it must not already exist")?;
    let config_path = notes_directory.join("config.json");
    let version_path = root.join("VERSION");
    let mut config_created = false;
    let mut version_created = false;
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&config_path)?;
        config_created = true;
        file.write_all(content.as_bytes())?;
        if let Some(version) = initial_version {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&version_path)?;
            version_created = true;
            file.write_all(format!("{version}\n").as_bytes())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        let mut failures = Vec::new();
        for (path, created) in [
            (&config_path, config_created),
            (&version_path, version_created),
        ] {
            if created && let Err(cleanup) = fs::remove_file(path) {
                failures.push(format!("{}: {cleanup}", path.display()));
            }
        }
        if let Err(cleanup) = fs::remove_dir(&notes_directory) {
            failures.push(format!("{}: {cleanup}", notes_directory.display()));
        }
        if !failures.is_empty() {
            return Err(error.context(format!(
                "initialization cleanup failed: {}",
                failures.join("; ")
            )));
        }
        return Err(error);
    }
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
    safe_name(name)?;
    ensure!(
        !["README", "AGENTS", "CLAUDE", "GEMINI"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n)),
        "reserved note name"
    );
    let content = note_content(root, packages, message, kind)?;
    Ok(write_note(root, name, &content)?)
}

fn write_note(root: &Path, name: &str, content: &str) -> std::io::Result<PathBuf> {
    let path = root.join(".changenote").join(format!("{name}.md"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(content.as_bytes())?;
    Ok(path)
}

fn note_content(
    root: &Path,
    packages: Vec<(String, Bump)>,
    message: &str,
    kind: Option<&str>,
) -> Result<String> {
    ensure!(
        !packages.is_empty() && !message.trim().is_empty(),
        "packages and message are required"
    );
    let release_config = load_release_config(root)?;
    if let Some(kind) = kind {
        safe_name(kind)?;
        if let Some(changelog) = &release_config.changelog {
            changelog.validate_type(kind)?;
        }
    }
    let config = release_config.config;
    let mut changes = BTreeMap::new();
    for (pkg, bump) in packages {
        ensure!(config.packages.contains_key(&pkg), "unknown package {pkg}");
        if let Some(previous) = changes.insert(pkg.clone(), bump) {
            ensure!(previous == bump, "conflicting bumps for package {pkg}");
        }
    }
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

pub fn version(root: &Path) -> Result<Vec<Release>> {
    version_with_legacy_output(root, false)
}

/// Reconstruct pre-grouped-changelog release trees for publication verification.
/// Normal releases must use `version` instead.
#[doc(hidden)]
pub fn version_with_legacy_output(root: &Path, legacy: bool) -> Result<Vec<Release>> {
    version_with_output(root, legacy, !legacy)
}

/// Reconstruct historical release trees without lockfile synchronization.
#[doc(hidden)]
pub fn version_with_output(
    root: &Path,
    legacy: bool,
    sync_lockfiles: bool,
) -> Result<Vec<Release>> {
    let release_config = load_release_config(root)?;
    let notes = notes(root, &release_config)?;
    let details = planning::plan(root, &release_config, &notes)?;
    let releases: Vec<_> = details.iter().map(|d| d.detail.release.clone()).collect();
    ensure!(
        !legacy
            || !releases
                .iter()
                .any(|r| release_config.fixed.iter().any(|g| g.contains(&r.package))),
        "legacy output does not support active fixed groups"
    );
    let config = &release_config.config;
    if releases.is_empty() {
        return Ok(releases);
    }
    // Prepare every output before changing any input.
    let version_paths = release_version_paths(root, &release_config, &releases)?;
    let locks = if sync_lockfiles {
        lockfile::discover_paths(root, &version_paths)?
    } else {
        Vec::new()
    };
    let mut writes = Vec::new();
    let mut manifests = Vec::new();
    for detail in &details {
        let release = &detail.detail.release;
        let pkg = &config.packages[&release.package];
        let path = checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
        let old = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        writes.push((
            path,
            changelog::render_sections(release, &old, legacy, &detail.note_sections),
        ));
        for version_path in
            release_version_paths(root, &release_config, std::slice::from_ref(release))?
        {
            let content = version_content(&version_path, &release.version, legacy)?;
            manifests.push((version_path, content));
        }
    }
    writes.push((
        checked_path(root, Path::new(".changenote/releases.json"))?,
        format!(
            "{}\n",
            serde_json::to_string_pretty(
                &releases.iter().map(SavedRelease::from).collect::<Vec<_>>()
            )?
        ),
    ));
    let paths: Vec<_> = version_paths
        .into_iter()
        .chain(locks.iter().map(|lock| lock.path.clone()))
        .chain(writes.iter().map(|(path, _)| path.clone()))
        .chain(notes.iter().map(|(path, _)| path.clone()))
        .collect();
    transaction::apply(&paths, || {
        lockfile::apply(&manifests, &locks)?;
        for (path, content) in writes {
            fs::write(path, content)?;
        }
        for (path, _) in notes {
            fs::remove_file(path)?;
        }
        Ok(())
    })?;
    Ok(releases)
}

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").current_dir(root).args(args).output()?;
    ensure!(
        output.status.success(),
        "git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

pub fn tag(root: &Path) -> Result<Vec<String>> {
    let release_config = load_release_config(root)?;
    let config = &release_config.config;
    ensure!(
        git(root, &["status", "--porcelain"])?.is_empty(),
        "commit all changes before tagging"
    );
    let releases: Vec<SavedRelease> = serde_json::from_str(&fs::read_to_string(checked_path(
        root,
        Path::new(".changenote/releases.json"),
    )?)?)?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let existing_tags = git(
        root,
        &["for-each-ref", "--format=%(refname:strip=2)", "refs/tags"],
    )?;
    let mut pending = Vec::new();
    let mut tags = std::collections::BTreeSet::new();
    for release in releases {
        let pkg = config
            .packages
            .get(&release.package)
            .context("unknown release package")?;
        ensure!(
            read_version(&root.join(&pkg.path).join(&pkg.version_file))? == release.version,
            "release version does not match version file"
        );
        validate_tag(&release.tag)?;
        let expected = release_tag(&release_config, &release.package, &release.version)?;
        ensure!(release.tag == expected, "invalid release tag");
        reserve_release_tag(&mut tags, &release.tag)?;
        for existing in existing_tags.lines() {
            ensure!(
                !tag_namespace_conflict(&release.tag, existing),
                "release tag namespace conflicts with existing tag: {} and {existing}",
                release.tag
            );
        }
        let existing = git(root, &["tag", "--list", &release.tag])?;
        if !existing.is_empty() {
            ensure!(
                git(
                    root,
                    &[
                        "rev-parse",
                        &format!("refs/tags/{}^{{commit}}", release.tag)
                    ]
                )? == head,
                "tag already exists on a different commit: {}",
                release.tag
            );
        } else {
            pending.push(release.tag);
        }
    }
    for tag in &pending {
        git(root, &["tag", "-a", tag, "-m", tag])?;
    }
    Ok(pending)
}
