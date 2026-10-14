use crate::{load_release_config, notes, plan};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Component, Path, PathBuf},
    process::Command,
};

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub merge_base: String,
    pub changed_packages: Vec<String>,
    pub note_paths: Vec<PathBuf>,
    pub outside_paths: Vec<PathBuf>,
    pub skip_reason: Option<String>,
    pub passed: bool,
}

/// Check committed changes only. An exemption never bypasses input validation.
pub fn check(
    root: &Path,
    since: &str,
    skip_release: bool,
    reason: Option<&str>,
) -> Result<CheckReport> {
    ensure!(
        skip_release == reason.is_some(),
        "--skip-release requires --reason and vice versa"
    );
    if let Some(reason) = reason {
        ensure!(!reason.trim().is_empty(), "reason cannot be empty");
    }
    let root = root.canonicalize()?;
    let prefix = git(&root, &["rev-parse", "--show-prefix"])?;
    ensure!(
        prefix == b"\n" || prefix == b"\r\n",
        "check root must be the Git repository root"
    );
    ensure!(
        git(
            &root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"]
        )?
        .is_empty(),
        "check requires a clean working tree; commit tracked and untracked changes first"
    );
    let release_config = load_release_config(&root)?;
    let config = &release_config.config;
    let notes = notes(&root, config)?;
    // Also validate version arithmetic, even for exempt PRs.
    plan(&root, &release_config, &notes)?;
    let reference = String::from_utf8(git(
        &root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{since}^{{commit}}"),
        ],
    )?)?;
    let base = git(&root, &["merge-base", reference.trim(), "HEAD"])
        .context("cannot find merge base; fetch full history and the base ref")?;
    let base = String::from_utf8(base)?.trim().to_owned();
    // No renames means both old and new paths are reported. Deleted notes are
    // excluded below because only notes that remain in HEAD can satisfy the rule.
    let changed = git(
        &root,
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--name-only",
            "-z",
            &base,
            "HEAD",
            "--",
        ],
    )?;
    let mut packages = BTreeSet::new();
    let mut note_paths = BTreeSet::new();
    let mut outside = BTreeSet::new();
    for bytes in changed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = path_from_bytes(bytes)?;
        if notes.iter().any(|(p, _)| *p == root.join(&path)) {
            note_paths.insert(path.clone());
        }
        let owner = config
            .packages
            .iter()
            .filter_map(|(name, pkg)| {
                let directory: PathBuf = pkg
                    .path
                    .components()
                    .filter(|c| !matches!(c, Component::CurDir))
                    .collect();
                path.starts_with(&directory)
                    .then_some((directory.components().count(), name))
            })
            .max_by_key(|(depth, _)| *depth);
        if let Some((_, name)) = owner {
            packages.insert(name.clone());
        } else {
            outside.insert(path);
        }
    }
    let passed = packages.is_empty() || !note_paths.is_empty() || skip_release;
    Ok(CheckReport {
        merge_base: base,
        changed_packages: packages.into_iter().collect(),
        note_paths: note_paths.into_iter().collect(),
        outside_paths: outside.into_iter().collect(),
        skip_reason: reason.map(|r| r.trim().to_owned()),
        passed,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .context("cannot run Git")?;
    ensure!(
        output.status.success(),
        "Git {} failed: {}",
        args[0],
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(bytes)))
}
#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::str::from_utf8(bytes).context("Git path is not UTF-8")?,
    ))
}
