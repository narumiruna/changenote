use crate::{Bump, Release};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangelogConfig {
    pub sections: Vec<SectionConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SectionConfig {
    pub title: String,
    pub types: Vec<String>,
}

/// One nonempty changelog section, in output order.
#[derive(Debug, Serialize)]
pub struct NoteSection {
    pub title: String,
    pub notes: Vec<String>,
}

impl ChangelogConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.sections.is_empty(),
            "changelog sections cannot be empty"
        );
        let mut titles = BTreeSet::from(["Other Changes"]);
        let mut types = BTreeSet::new();
        for section in &self.sections {
            ensure!(
                !section.title.trim().is_empty()
                    && section.title.trim() == section.title
                    && !section.title.chars().any(char::is_control),
                "section title must be nonempty, trimmed, and contain no control characters"
            );
            ensure!(
                semver::Version::parse(section.title.split_whitespace().next().unwrap()).is_err(),
                "section title cannot start with a SemVer version"
            );
            ensure!(
                titles.insert(&section.title),
                "duplicate or reserved section title"
            );
            ensure!(!section.types.is_empty(), "section types cannot be empty");
            for kind in &section.types {
                crate::safe_name(kind)?;
                ensure!(types.insert(kind), "duplicate section type {kind}");
            }
        }
        Ok(())
    }

    pub fn validate_type(&self, kind: &str) -> Result<()> {
        ensure!(
            self.sections
                .iter()
                .any(|s| s.types.iter().any(|t| t == kind)),
            "unknown note type {kind}"
        );
        Ok(())
    }
}

pub(crate) fn group(
    release: &Release,
    types: &[Option<String>],
    config: Option<&ChangelogConfig>,
) -> Vec<NoteSection> {
    let mut result = Vec::new();
    let mut push = |title: String, notes: Vec<String>| {
        if !notes.is_empty() {
            result.push(NoteSection { title, notes });
        }
    };
    if let Some(config) = config {
        for section in &config.sections {
            push(
                section.title.clone(),
                release
                    .notes
                    .iter()
                    .zip(types)
                    .filter(|(_, t)| t.as_ref().is_some_and(|t| section.types.contains(t)))
                    .map(|(n, _)| n.clone())
                    .collect(),
            );
        }
        push(
            "Other Changes".into(),
            release
                .notes
                .iter()
                .zip(types)
                .filter(|(_, t)| t.is_none())
                .map(|(n, _)| n.clone())
                .collect(),
        );
    } else {
        for (bump, title) in [
            (Bump::Major, "Major Changes"),
            (Bump::Minor, "Minor Changes"),
            (Bump::Patch, "Patch Changes"),
        ] {
            push(
                title.into(),
                release
                    .notes
                    .iter()
                    .zip(&release.note_bumps)
                    .filter(|(_, b)| **b == bump)
                    .map(|(n, _)| n.clone())
                    .collect(),
            );
        }
    }
    result
}
