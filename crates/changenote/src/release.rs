//! Release plans, version application, changelog extraction, and local tags.
use crate::config::ReleaseConfig;
use crate::note::NoteData;
use crate::{
    changelog, config, lockfile, note, paths, planning, sections, transaction, version_file,
};
use anyhow::{Result, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Bump {
    Patch,
    Minor,
    Major,
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
    use anyhow::Context;

    let config = config::load_release_config(root)?;
    ensure!(
        sections::enabled(config.changelog.as_ref()),
        "GitHub release notes are unavailable because changelog output is disabled"
    );
    let pkg = config
        .config
        .packages
        .get(package)
        .context("unknown release package")?;
    let path = paths::checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
    ensure!(
        !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "changelog cannot be a symlink"
    );
    changelog::extract(&fs::read_to_string(path)?, version)
}

// Match Git's full ref-name rules without requiring a Git executable for planning.
pub(crate) fn validate_tag(tag: &str) -> Result<()> {
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

pub(crate) fn reserve_release_tag(tags: &mut BTreeSet<String>, tag: &str) -> Result<()> {
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

pub(crate) fn release_tag(
    config: &ReleaseConfig,
    package: &str,
    version: &Version,
) -> Result<String> {
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

pub(crate) fn plan(
    root: &Path,
    config: &ReleaseConfig,
    notes: &[(PathBuf, NoteData)],
) -> Result<Vec<Release>> {
    Ok(planning::plan(root, config, notes)?
        .into_iter()
        .map(|r| r.sectioned.detail.release)
        .collect())
}

pub fn status(root: &Path) -> Result<Vec<Release>> {
    let config = config::load_release_config(root)?;
    plan(root, &config, &note::notes(root, &config)?)
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
    let release_config = config::load_release_config(root)?;
    let notes = note::notes(root, &release_config)?;
    let details = planning::plan_with_sources(root, &release_config, &notes, true)?;
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
    let released: BTreeSet<_> = releases.iter().map(|release| &release.package).collect();
    let consumed: Vec<_> = notes
        .iter()
        .filter(|(_, note)| note.packages.keys().any(|name| released.contains(name)))
        .map(|(path, _)| path.clone())
        .collect();
    // Prepare every output before changing any input.
    let version_paths = config::release_version_paths(root, &release_config, &releases)?;
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
        if sections::enabled(release_config.changelog.as_ref()) {
            let path = paths::checked_path(root, &pkg.path.join("CHANGELOG.md"))?;
            let old = if path.exists() {
                fs::read_to_string(&path)?
            } else {
                String::new()
            };
            writes.push((
                path,
                changelog::render_sections(release, &old, legacy, &detail.note_sections),
            ));
        }
        for version_path in
            config::release_version_paths(root, &release_config, std::slice::from_ref(release))?
        {
            let content = version_file::version_content(&version_path, &release.version, legacy)?;
            manifests.push((version_path, content));
        }
    }
    writes.push((
        paths::checked_path(root, Path::new(".changenote/releases.json"))?,
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
        for path in consumed {
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
    use anyhow::Context;

    let release_config = config::load_release_config(root)?;
    let config = &release_config.config;
    ensure!(
        git(root, &["status", "--porcelain"])?.is_empty(),
        "commit all changes before tagging"
    );
    let releases: Vec<SavedRelease> = serde_json::from_str(&fs::read_to_string(
        paths::checked_path(root, Path::new(".changenote/releases.json"))?,
    )?)?;
    let head = git(root, &["rev-parse", "HEAD"])?;
    let existing_tags = git(
        root,
        &["for-each-ref", "--format=%(refname:strip=2)", "refs/tags"],
    )?;
    let mut pending = Vec::new();
    let mut tags = BTreeSet::new();
    for release in releases {
        let pkg = config
            .packages
            .get(&release.package)
            .context("unknown release package")?;
        ensure!(
            version_file::read_version(&root.join(&pkg.path).join(&pkg.version_file))?
                == release.version,
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
