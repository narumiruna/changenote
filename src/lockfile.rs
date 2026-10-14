//! Refresh existing lockfiles after all configured manifests have been updated.
use crate::{Config, Release, checked_path};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug)]
pub(crate) struct Lockfile {
    pub path: PathBuf,
    tool: &'static str,
    args: &'static [&'static str],
}

pub(crate) fn discover(
    root: &Path,
    config: &Config,
    releases: &[Release],
) -> Result<Vec<Lockfile>> {
    let root = root.canonicalize()?;
    let mut locks = BTreeMap::new();
    for release in releases {
        let package = &config.packages[&release.package];
        let (name, tool, args): (_, _, &[&str]) =
            match package.version_file.file_name().and_then(|s| s.to_str()) {
                Some("Cargo.toml") => (
                    "Cargo.lock",
                    "cargo",
                    &["update", "--workspace", "--offline"],
                ),
                Some("pyproject.toml") => ("uv.lock", "uv", &["lock", "--offline"]),
                Some("package.json") => (
                    "package-lock.json",
                    "npm",
                    &[
                        "install",
                        "--package-lock-only",
                        "--ignore-scripts",
                        "--offline",
                        "--no-audit",
                        "--no-fund",
                    ],
                ),
                _ => continue,
            };
        let manifest = checked_path(&root, &package.path.join(&package.version_file))?;
        let mut directory = manifest
            .parent()
            .context("missing manifest directory")?
            .to_path_buf();
        loop {
            let path = directory.join(name);
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    ensure!(
                        metadata.is_file() && !metadata.file_type().is_symlink(),
                        "lockfile must be a regular file, not a symlink: {}",
                        path.display()
                    );
                    let relative = path.strip_prefix(&root)?;
                    checked_path(&root, relative)?;
                    let owner = directory.join(package.version_file.file_name().unwrap());
                    ensure!(
                        owner.is_file() && !fs::symlink_metadata(&owner)?.file_type().is_symlink(),
                        "lockfile manifest must be a regular file: {}",
                        owner.display()
                    );
                    ensure!(
                        directory.canonicalize()?.starts_with(&root),
                        "lockfile directory escapes repository"
                    );
                    if tool == "npm" {
                        ensure!(
                            !directory.join("npm-shrinkwrap.json").try_exists()?,
                            "npm-shrinkwrap.json is not supported: {}",
                            directory.display()
                        );
                    }
                    let path = path.canonicalize()?;
                    locks.insert(path.clone(), Lockfile { path, tool, args });
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            if directory == root {
                break;
            }
            directory = directory
                .parent()
                .context("lockfile directory escapes repository")?
                .to_path_buf();
        }
    }
    Ok(locks.into_values().collect())
}

impl Lockfile {
    fn refresh(&self) -> Result<()> {
        let output = Command::new(self.tool)
            .args(self.args)
            .current_dir(self.path.parent().unwrap())
            .output()
            .with_context(|| format!("cannot run {} for {}", self.tool, self.path.display()))?;
        ensure!(
            output.status.success(),
            "{} failed for {}: {}{}",
            self.tool,
            self.path.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }
}

/// Restore manifests and lockfiles if a package manager fails. Notes stay intact.
pub(crate) fn apply(manifests: &[(PathBuf, String)], locks: &[Lockfile]) -> Result<()> {
    let originals = manifests
        .iter()
        .map(|(path, _)| path)
        .chain(locks.iter().map(|lock| &lock.path))
        .map(|path| Ok((path, fs::read(path)?)))
        .collect::<Result<Vec<_>>>()?;
    let result = (|| {
        for (path, content) in manifests {
            fs::write(path, content)?;
        }
        for lock in locks {
            lock.refresh()?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for (path, content) in originals {
            fs::write(path, content).with_context(|| {
                format!(
                    "lockfile sync failed ({error:#}); cannot restore {}",
                    path.display()
                )
            })?;
        }
        return Err(error);
    }
    Ok(())
}
