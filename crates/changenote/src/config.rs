//! Configuration models, validation, and release output paths.
use crate::paths;
use crate::patterns;
use crate::patterns::FilePatterns;
use crate::release;
use crate::release::Release;
use crate::sections;
use crate::sections::ChangelogConfig;
use crate::version_file;
use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(rename = "$schema")]
    schema: Option<String>,
    packages: BTreeMap<String, PackageFile>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    base_branch: Option<String>,
    #[serde(default)]
    changed_file_patterns: FilePatterns,
    #[serde(default)]
    fixed: Vec<Vec<String>>,
    #[serde(default)]
    linked: Vec<Vec<String>>,
    #[serde(default)]
    ignore: Vec<String>,
    changelog: Option<ChangelogConfig>,
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

fn deserialize_optional_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

pub(crate) fn deserialize_optional_value<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

pub(crate) struct PackageOptions {
    pub(crate) version_outputs: Vec<PathBuf>,
    pub(crate) tag_prefix: Option<String>,
}

pub(crate) struct ReleaseConfig {
    pub(crate) config: Config,
    pub(crate) base_branch: Option<String>,
    pub(crate) changed_file_patterns: FilePatterns,
    pub(crate) fixed: Vec<Vec<String>>,
    pub(crate) linked: Vec<Vec<String>>,
    pub(crate) ignore: BTreeSet<String>,
    pub(crate) changelog: Option<ChangelogConfig>,
    pub(crate) package_options: BTreeMap<String, PackageOptions>,
}

/// Validate all configuration and return only the primary package settings.
/// Extended settings, including the check base ref, are not retained in this view.
pub fn load_config(root: &Path) -> Result<Config> {
    Ok(load_release_config(root)?.config)
}

pub(crate) fn load_release_config(root: &Path) -> Result<ReleaseConfig> {
    use anyhow::Context;

    let path = paths::checked_path(root, Path::new(".changenote/config.json"))?;
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
    let fixed = patterns::expand_fixed(
        &file.fixed,
        &config
            .packages
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    )?;
    let mut ignore = BTreeSet::new();
    for name in file.ignore {
        ensure!(
            config.packages.contains_key(&name),
            "unknown ignored package {name}"
        );
        ensure!(
            ignore.insert(name.clone()),
            "duplicate ignored package {name}"
        );
    }
    for group in &fixed {
        let paused = group.iter().filter(|name| ignore.contains(*name)).count();
        ensure!(
            paused == 0 || paused == group.len(),
            "ignore cannot pause part of a fixed group"
        );
    }
    let mut members = BTreeSet::new();
    for group in &fixed {
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
    for group in &file.linked {
        ensure!(
            group.len() >= 2,
            "linked groups require at least two packages"
        );
        for name in group {
            ensure!(
                config.packages.contains_key(name),
                "unknown linked package {name}"
            );
            ensure!(
                members.insert(name),
                "duplicate linked or fixed membership: {name}"
            );
        }
    }
    ensure!(
        !config.packages.is_empty(),
        "config must contain at least one package"
    );
    let write_changelogs = sections::enabled(file.changelog.as_ref());
    let mut files = BTreeSet::new();
    for (name, pkg) in &config.packages {
        paths::safe_name(name)?;
        paths::relative(&pkg.path)?;
        paths::relative(&pkg.version_file)?;
        let directory = paths::checked_path(root, &pkg.path)?;
        ensure!(
            directory.is_dir(),
            "package directory does not exist: {}",
            directory.display()
        );
        let file = paths::checked_path(root, &pkg.path.join(&pkg.version_file))?;
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
        if write_changelogs {
            let changelog = paths::checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
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
        }
        let version = version_file::read_version(&file)?;
        let options = &package_options[name];
        if let Some(prefix) = &options.tag_prefix {
            release::validate_tag(&format!("{prefix}{version}"))?;
        }
        for output in &options.version_outputs {
            paths::relative(output)?;
            let output = paths::checked_path(root, &pkg.path.join(output))?;
            paths::reject_output_symlinks(root, &output)?;
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
                version_file::read_version(&output)? == version,
                "version output does not match primary version: {}",
                output.display()
            );
        }
    }
    Ok(ReleaseConfig {
        config,
        base_branch: file.base_branch,
        changed_file_patterns: file.changed_file_patterns,
        fixed,
        linked: file.linked,
        ignore,
        changelog: file.changelog,
        package_options,
    })
}

/// Return checked absolute paths for every primary and additional release version file.
pub(crate) fn release_version_paths(
    root: &Path,
    config: &ReleaseConfig,
    releases: &[Release],
) -> Result<Vec<PathBuf>> {
    use anyhow::Context;

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
            paths.push(paths::checked_path(root, &package.path.join(path))?);
        }
    }
    Ok(paths)
}
