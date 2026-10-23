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

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Coverage {
    Pr,
    Package,
}

#[derive(Debug, Serialize)]
pub struct CoverageReport {
    #[serde(flatten)]
    pub report: CheckReport,
    pub covered_packages: Vec<String>,
    pub uncovered_packages: Vec<String>,
}

/// Check committed changes only, with the original PR-level report.
pub fn check(
    root: &Path,
    since: &str,
    skip_release: bool,
    reason: Option<&str>,
) -> Result<CheckReport> {
    Ok(check_with_coverage(root, since, skip_release, reason, Coverage::Pr)?.report)
}

/// Check direct assignments in changed notes. Exemptions still validate inputs.
pub fn check_with_coverage(
    root: &Path,
    since: &str,
    skip_release: bool,
    reason: Option<&str>,
    coverage: Coverage,
) -> Result<CoverageReport> {
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
    let notes = notes(&root, &release_config)?;
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
    let mut assignments = BTreeSet::new();
    let mut outside = BTreeSet::new();
    for bytes in changed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = path_from_bytes(bytes)?;
        if let Some((_, note)) = notes.iter().find(|(p, _)| *p == root.join(&path)) {
            note_paths.insert(path.clone());
            assignments.extend(note.packages.keys().cloned());
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
    let covered_packages = packages.intersection(&assignments).cloned().collect();
    let uncovered_packages: Vec<_> = packages.difference(&assignments).cloned().collect();
    let passed = skip_release
        || match coverage {
            Coverage::Pr => packages.is_empty() || !note_paths.is_empty(),
            Coverage::Package => uncovered_packages.is_empty(),
        };
    Ok(CoverageReport {
        report: CheckReport {
            merge_base: base,
            changed_packages: packages.into_iter().collect(),
            note_paths: note_paths.into_iter().collect(),
            outside_paths: outside.into_iter().collect(),
            skip_reason: reason.map(|r| r.trim().to_owned()),
            passed,
        },
        covered_packages,
        uncovered_packages,
    })
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", "core.fsmonitor=false"])
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
