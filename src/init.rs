//! Initialize configuration from a new or existing version source.
use crate::config::{Config, Package};
use crate::{paths, version_file};
use anyhow::{Context, Result, ensure};
use semver::Version;
use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

pub fn init(root: &Path, name: &str, version: Version) -> Result<()> {
    paths::safe_name(name)?;
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
    paths::safe_name(name)?;
    let source = paths::checked_path(root, version_file)?;
    paths::reject_output_symlinks(root, &source)?;
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
    version_file::read_version(&source)?;
    let directory = version_file.parent().context("missing version directory")?;
    let changelog = paths::checked_path(root, &directory.join("CHANGELOG.md"))?;
    match fs::symlink_metadata(&changelog) {
        Ok(metadata) => ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "changelog must be a regular file, not a symlink"
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
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
    use std::io::Write;

    let directory = version_file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let config = Config {
        schema: Some(
            "https://raw.githubusercontent.com/changenote/changenote/main/schema.json".to_owned(),
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
