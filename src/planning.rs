use crate::version_file::read_version;
use crate::{Bump, NoteData, NoteSection, Release, ReleaseConfig, load_release_config, notes};
use anyhow::{Context, Result};
use semver::Version;
use serde::{Serialize, Serializer, ser::Error};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// A release with direct note provenance. Default and saved batches omit this metadata.
#[derive(Debug, Serialize)]
pub struct DetailedRelease {
    #[serde(flatten)]
    pub release: Release,
    #[serde(serialize_with = "serialize_paths")]
    pub note_paths: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_group: Option<FixedGroup>,
}

/// A release with direct note provenance and planned changelog sections.
#[derive(Debug, Serialize)]
pub struct SectionedRelease {
    #[serde(flatten)]
    pub detail: DetailedRelease,
    /// Type for each direct summary; omitted from default JSON.
    #[serde(skip)]
    pub note_types: Vec<Option<String>>,
    /// Nonempty sections in changelog output order; omitted from default JSON.
    #[serde(skip)]
    pub note_sections: Vec<NoteSection>,
}

/// Cause of a fixed-group release; source paths are not direct package notes.
#[derive(Debug, Clone, Serialize)]
pub struct FixedGroup {
    pub members: Vec<String>,
    pub base_version: Version,
    #[serde(serialize_with = "serialize_paths")]
    pub source_note_paths: Vec<PathBuf>,
}

pub fn status_with_note_paths(root: &Path) -> Result<Vec<DetailedRelease>> {
    Ok(status_with_note_sections(root)?
        .into_iter()
        .map(|r| r.detail)
        .collect())
}

pub fn status_with_note_sections(root: &Path) -> Result<Vec<SectionedRelease>> {
    let config = load_release_config(root)?;
    plan(root, &config, &notes(root, &config)?)
}

fn bump_version(mut version: Version, bump: Bump) -> Result<Version> {
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
    Ok(version)
}

pub(crate) fn plan(
    root: &Path,
    config: &ReleaseConfig,
    notes: &[(PathBuf, NoteData)],
) -> Result<Vec<SectionedRelease>> {
    let versions = config
        .config
        .packages
        .iter()
        .map(|(name, package)| {
            Ok((
                name.clone(),
                read_version(&root.join(&package.path).join(&package.version_file))?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut targets = BTreeMap::new();
    for (name, current) in &versions {
        if config.fixed.iter().any(|g| g.contains(name)) {
            continue;
        }
        if let Some(bump) = notes.iter().filter_map(|(_, n)| n.packages.get(name)).max() {
            targets.insert(
                name.clone(),
                (*bump, bump_version(current.clone(), *bump)?, None),
            );
        }
    }
    for group in &config.fixed {
        let sources: Vec<_> = notes
            .iter()
            .filter(|(_, n)| group.iter().any(|p| n.packages.contains_key(p)))
            .collect();
        let Some(bump) = sources
            .iter()
            .flat_map(|(_, n)| group.iter().filter_map(|p| n.packages.get(p)))
            .max()
            .copied()
        else {
            continue;
        };
        let base = group.iter().map(|p| &versions[p]).max().unwrap().clone();
        let version = bump_version(base.clone(), bump)?;
        let mut members = group.clone();
        members.sort();
        let cause = FixedGroup {
            members,
            base_version: base,
            source_note_paths: sources.iter().map(|(p, _)| relative_note(p)).collect(),
        };
        for name in group {
            targets.insert(name.clone(), (bump, version.clone(), Some(cause.clone())));
        }
    }
    let mut tags = std::collections::BTreeSet::new();
    targets
        .into_iter()
        .map(|(name, (bump, version, fixed_group))| {
            let direct: Vec<_> = notes
                .iter()
                .filter_map(|(p, n)| n.packages.get(&name).map(|b| (p, n, *b)))
                .collect();
            let tag = crate::release_tag(config, &name, &version)?;
            crate::reserve_release_tag(&mut tags, &tag)?;
            let note_types = direct
                .iter()
                .map(|(_, n, _)| n.kind.clone())
                .collect::<Vec<_>>();
            let release = Release {
                old_version: versions[&name].clone(),
                package: name,
                version,
                bump,
                tag,
                notes: direct.iter().map(|(_, n, _)| n.summary.clone()).collect(),
                note_bumps: direct.iter().map(|(_, _, b)| *b).collect(),
            };
            let note_sections =
                crate::sections::group(&release, &note_types, config.changelog.as_ref());
            Ok(SectionedRelease {
                note_types,
                note_sections,
                detail: DetailedRelease {
                    note_paths: direct.iter().map(|(p, _, _)| relative_note(p)).collect(),
                    fixed_group,
                    release,
                },
            })
        })
        .collect()
}

// JSON uses forward slashes on every platform; Rust callers retain native paths.
fn serialize_paths<S: Serializer>(paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error> {
    paths
        .iter()
        .map(|p| {
            let text = p
                .to_str()
                .ok_or_else(|| S::Error::custom("note path is not UTF-8"))?;
            Ok(if cfg!(windows) {
                text.replace('\\', "/")
            } else {
                text.to_owned()
            })
        })
        .collect::<Result<Vec<_>, S::Error>>()?
        .serialize(serializer)
}

fn relative_note(path: &Path) -> PathBuf {
    Path::new(".changenote").join(path.file_name().unwrap())
}
