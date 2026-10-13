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

#[derive(Debug, Deserialize, Serialize)]
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

pub fn load_config(root: &Path) -> Result<Config> {
    let path = checked_path(root, Path::new(".changenote/config.json"))?;
    let config: Config = serde_json::from_str(&fs::read_to_string(path)?)?;
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
        read_version(&file)?;
    }
    Ok(config)
}

pub fn parse_note(content: &str) -> Result<Note> {
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
    let value: serde_yaml::Value = serde_yaml::from_str(&yaml)?;
    let packages: BTreeMap<String, Bump> = serde_yaml::from_value(value)?;
    ensure!(!packages.is_empty(), "note must name at least one package");
    let summary = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    ensure!(!summary.is_empty(), "note summary cannot be empty");
    Ok(Note { packages, summary })
}

fn notes(root: &Path, config: &Config) -> Result<Vec<(PathBuf, Note)>> {
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
        let note = parse_note(&fs::read_to_string(&path)?)
            .with_context(|| format!("invalid note {}", path.display()))?;
        for name in note.packages.keys() {
            ensure!(
                config.packages.contains_key(name),
                "unknown package {name} in {}",
                path.display()
            );
        }
        result.push((path, note));
    }
    Ok(result)
}

fn plan(root: &Path, config: &Config, notes: &[(PathBuf, Note)]) -> Result<Vec<Release>> {
    let mut releases = Vec::new();
    for (name, pkg) in &config.packages {
        let changes: Vec<_> = notes
            .iter()
            .filter_map(|(_, n)| n.packages.get(name).map(|b| (*b, n.summary.clone())))
            .collect();
        let Some(bump) = changes.iter().map(|(b, _)| *b).max() else {
            continue;
        };
        let old_version = read_version(&root.join(&pkg.path).join(&pkg.version_file))?;
        let mut version = old_version.clone();
        match bump {
            Bump::Major => {
                version.major = version.major.checked_add(1).context("version overflow")?;
                version.minor = 0;
                version.patch = 0;
            }
            Bump::Minor => {
                version.minor = version.minor.checked_add(1).context("version overflow")?;
                version.patch = 0;
            }
            Bump::Patch => {
                version.patch = version.patch.checked_add(1).context("version overflow")?;
            }
        }
        let tag = if config.packages.len() == 1 {
            format!("v{version}")
        } else {
            format!("changenote/{name}@v{version}")
        };
        releases.push(Release {
            package: name.clone(),
            old_version,
            version,
            bump,
            tag,
            note_bumps: changes.iter().map(|(b, _)| *b).collect(),
            notes: changes.into_iter().map(|(_, s)| s).collect(),
        });
    }
    Ok(releases)
}

pub fn status(root: &Path) -> Result<Vec<Release>> {
    let config = load_config(root)?;
    plan(root, &config, &notes(root, &config)?)
}

pub fn init(root: &Path, name: &str, version: Version) -> Result<()> {
    safe_name(name)?;
    ensure!(
        version.pre.is_empty() && version.build.is_empty(),
        "only stable SemVer versions are supported"
    );
    ensure!(
        !root.join(".changenote").exists(),
        ".changenote already exists"
    );
    ensure!(
        !root.join("VERSION").exists(),
        "VERSION already exists; configure it manually"
    );
    fs::create_dir(root.join(".changenote"))?;
    let config = Config {
        schema: Some(
            "https://raw.githubusercontent.com/narumiruna/changenote/main/schema.json".to_owned(),
        ),
        packages: BTreeMap::from([(
            name.to_owned(),
            Package {
                path: ".".into(),
                version_file: "VERSION".into(),
            },
        )]),
    };
    fs::write(
        root.join(".changenote/config.json"),
        format!("{}\n", serde_json::to_string_pretty(&config)?),
    )?;
    fs::write(root.join("VERSION"), format!("{version}\n"))?;
    Ok(())
}

pub fn add(
    root: &Path,
    name: &str,
    packages: Vec<String>,
    bump: Bump,
    message: &str,
) -> Result<PathBuf> {
    safe_name(name)?;
    ensure!(
        !["README", "AGENTS", "CLAUDE", "GEMINI"]
            .iter()
            .any(|n| name.eq_ignore_ascii_case(n)),
        "reserved note name"
    );
    ensure!(
        !packages.is_empty() && !message.trim().is_empty(),
        "packages and message are required"
    );
    let config = load_config(root)?;
    let mut changes = BTreeMap::new();
    for pkg in packages {
        ensure!(config.packages.contains_key(&pkg), "unknown package {pkg}");
        changes.insert(pkg, bump);
    }
    let content = format!(
        "---\n{}---\n\n{}\n",
        serde_yaml::to_string(&changes)?,
        message.trim()
    );
    let path = root.join(".changenote").join(format!("{name}.md"));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?
        .write_all(content.as_bytes())?;
    Ok(path)
}

pub fn version(root: &Path) -> Result<Vec<Release>> {
    let config = load_config(root)?;
    let notes = notes(root, &config)?;
    let releases = plan(root, &config, &notes)?;
    if releases.is_empty() {
        return Ok(releases);
    }
    // Prepare every output before changing any input. Filesystem failures still require Git recovery.
    let mut writes = Vec::new();
    for release in &releases {
        let pkg = &config.packages[&release.package];
        let path = checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
        let old = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            String::new()
        };
        let entry = format!(
            "## {}\n\n{}\n\n",
            release.version,
            release
                .notes
                .iter()
                .map(|n| format!("- {}", n.replace('\n', "\n  ")))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
        writes.push((path, format!("{entry}{old}")));
        let version_path = checked_path(root, &pkg.path.join(&pkg.version_file))?;
        let content = version_content(&version_path, &release.version)?;
        writes.push((version_path, content));
    }
    writes.push((
        checked_path(root, Path::new(".changenote/releases.json"))?,
        format!("{}\n", serde_json::to_string_pretty(&releases)?),
    ));
    for (path, content) in writes {
        fs::write(path, content)?;
    }
    for (path, _) in notes {
        fs::remove_file(path)?;
    }
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
    let config = load_config(root)?;
    ensure!(
        git(root, &["status", "--porcelain"])?.is_empty(),
        "commit all changes before tagging"
    );
    let releases: Vec<Release> = serde_json::from_str(&fs::read_to_string(checked_path(
        root,
        Path::new(".changenote/releases.json"),
    )?)?)?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let mut pending = Vec::new();
    for release in releases {
        let pkg = config
            .packages
            .get(&release.package)
            .context("unknown release package")?;
        ensure!(
            read_version(&root.join(&pkg.path).join(&pkg.version_file))? == release.version,
            "release version does not match version file"
        );
        let expected = if config.packages.len() == 1 {
            format!("v{}", release.version)
        } else {
            format!("changenote/{}@v{}", release.package, release.version)
        };
        ensure!(release.tag == expected, "invalid release tag");
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
