//! Bounded note templates. Notes are inserted once and never interpreted.
use crate::{Bump, Release};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, de::Error};

#[derive(Debug)]
pub(crate) struct NoteTemplate(String);

impl<'de> Deserialize<'de> for NoteTemplate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl NoteTemplate {
    fn parse(value: &str) -> Result<Self> {
        if value == "{summary}" {
            return Ok(Self(String::new()));
        }
        let prefix = value.strip_suffix("\n\n{summary}")
            .context("note_template must be {summary} or one metadata line followed by a blank line and {summary}")?;
        ensure!(
            !prefix.is_empty()
                && prefix.trim() == prefix
                && !prefix.chars().any(char::is_control)
                && !prefix.starts_with("```")
                && !prefix.starts_with("~~~"),
            "note_template metadata must be one trimmed nonempty line and cannot open a code fence"
        );
        let literal = prefix
            .replace("{package}", "")
            .replace("{version}", "")
            .replace("{bump}", "");
        ensure!(
            !literal.contains(['{', '}']),
            "unknown or unbalanced note_template placeholder"
        );
        Ok(Self(prefix.into()))
    }

    pub(crate) fn render(&self, release: &Release, bump: Bump, summary: &str) -> String {
        if self.0.is_empty() {
            return summary.to_owned();
        }
        let bump = match bump {
            Bump::Major => "major",
            Bump::Minor => "minor",
            Bump::Patch => "patch",
        };
        let prefix = self
            .0
            .replace("{package}", &release.package)
            .replace("{version}", &release.version.to_string())
            .replace("{bump}", bump);
        format!("{prefix}\n\n{summary}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_templates_and_inserts_notes_as_literal_markdown() {
        let release = Release {
            package: "pkg".into(),
            old_version: "1.0.0".parse().unwrap(),
            version: "1.1.0".parse().unwrap(),
            bump: Bump::Minor,
            tag: "v1.1.0".into(),
            notes: vec![],
            note_bumps: vec![],
        };
        let note = "```rs\n{version} $(touch file)\n```\n\n## 9.9.9";
        assert_eq!(
            NoteTemplate::parse("{summary}")
                .unwrap()
                .render(&release, Bump::Patch, note),
            note
        );
        assert_eq!(
            NoteTemplate::parse("{package}@{version} [{bump}]\n\n{summary}")
                .unwrap()
                .render(&release, Bump::Patch, note),
            format!("pkg@1.1.0 [patch]\n\n{note}")
        );
    }

    #[test]
    fn invalid_templates_fail_before_rendering() {
        for value in [
            "",
            "{unknown}",
            "{package}: {summary}",
            "{summary}\n",
            "Meta\n{summary}",
            "Meta\n\n{summary}tail",
            "{summary}\n\n{summary}",
            "{unknown}\n\n{summary}",
            "Meta{\n\n{summary}",
            "Meta}\n\n{summary}",
            "Meta\tX\n\n{summary}",
            "Meta\nX\n\n{summary}",
            " Meta\n\n{summary}",
            "```rs\n\n{summary}",
            "~~~\n\n{summary}",
        ] {
            assert!(NoteTemplate::parse(value).is_err(), "{value:?}");
        }
        for value in [serde_json::Value::Null, true.into(), serde_json::json!([])] {
            assert!(serde_json::from_value::<NoteTemplate>(value).is_err());
        }
    }
}
