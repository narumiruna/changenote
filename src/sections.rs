use crate::{Bump, Release};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChangelogConfig {
    #[serde(default, deserialize_with = "crate::deserialize_optional_value")]
    pub sections: Option<Vec<SectionConfig>>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default, deserialize_with = "crate::deserialize_optional_value")]
    pub note_template: Option<crate::template::NoteTemplate>,
}

fn default_enabled() -> bool {
    true
}

pub(crate) fn enabled(config: Option<&ChangelogConfig>) -> bool {
    config.is_none_or(|config| config.enabled)
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
        let Some(sections) = &self.sections else {
            return Ok(());
        };
        ensure!(!sections.is_empty(), "changelog sections cannot be empty");
        let mut titles = BTreeSet::from(["Other Changes"]);
        let mut types = BTreeSet::new();
        for section in sections {
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
        let Some(sections) = &self.sections else {
            return Ok(());
        };
        ensure!(
            sections.iter().any(|s| s.types.iter().any(|t| t == kind)),
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
    let rendered = config
        .and_then(|config| config.note_template.as_ref())
        .map(|template| {
            let mut formatted = release.clone();
            formatted.notes = release
                .notes
                .iter()
                .enumerate()
                .map(|(index, note)| {
                    template.render(
                        release,
                        release
                            .note_bumps
                            .get(index)
                            .copied()
                            .unwrap_or(release.bump),
                        note,
                    )
                })
                .collect();
            formatted
        });
    let release = rendered.as_ref().unwrap_or(release);
    let mut result = Vec::new();
    let mut push = |title: String, notes: Vec<String>| {
        if !notes.is_empty() {
            result.push(NoteSection { title, notes });
        }
    };
    if let Some(sections) = config.and_then(|config| config.sections.as_ref()) {
        for section in sections {
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
