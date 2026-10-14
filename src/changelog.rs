use crate::{Bump, Release};
use semver::Version;

pub(crate) fn render(release: &Release, old: &str, legacy: bool) -> String {
    let mut entry = format!("## {}\n\n", release.version);
    if legacy {
        entry.push_str(&items(release.notes.iter().map(String::as_str)));
        entry.push_str("\n\n");
        return format!("{entry}{old}");
    }
    if release.notes.is_empty() {
        entry.push_str("Released with its fixed version group; no direct changes.\n\n");
    }
    for (bump, heading) in [
        (Bump::Major, "Major"),
        (Bump::Minor, "Minor"),
        (Bump::Patch, "Patch"),
    ] {
        let notes: Vec<_> = release
            .notes
            .iter()
            .zip(&release.note_bumps)
            .filter(|(_, b)| **b == bump)
            .map(|(n, _)| n.as_str())
            .collect();
        if !notes.is_empty() {
            entry.push_str(&format!(
                "### {heading} Changes\n\n{}\n\n",
                items(notes.into_iter())
            ));
        }
    }
    let index = first_version_heading(old).unwrap_or(old.len());
    let (prefix, suffix) = old.split_at(index);
    let separator = if prefix.is_empty() || prefix.ends_with("\n\n") || prefix.ends_with("\r\n\r\n")
    {
        ""
    } else if prefix.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{prefix}{separator}{entry}{suffix}")
}

fn items<'a>(notes: impl Iterator<Item = &'a str>) -> String {
    notes
        .map(|n| format!("- {}", n.replace('\n', "\n  ")))
        .collect::<Vec<_>>()
        .join("\n\n")
}

// Only ATX SemVer headings outside fenced or indented code start a release.
fn first_version_heading(text: &str) -> Option<usize> {
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    for line in text.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        let indent = text.bytes().take_while(|b| *b == b' ').count();
        if indent <= 3 {
            let text = &text[indent..];
            let marker = text.as_bytes().first().copied();
            if matches!(marker, Some(b'`' | b'~')) {
                let marker = marker.unwrap();
                let count = text.bytes().take_while(|b| *b == marker).count();
                if let Some((open, length)) = fence {
                    if marker == open && count >= length && text[count..].trim().is_empty() {
                        fence = None;
                    }
                } else if count >= 3 && (marker != b'`' || !text[count..].contains('`')) {
                    fence = Some((marker, count));
                }
            } else if fence.is_none() {
                let count = text.bytes().take_while(|b| *b == b'#').count();
                if (1..=6).contains(&count)
                    && text
                        .as_bytes()
                        .get(count)
                        .is_some_and(u8::is_ascii_whitespace)
                {
                    let version = text[count..].split_whitespace().next().unwrap_or("");
                    if Version::parse(version).is_ok() {
                        return Some(offset);
                    }
                }
            }
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release() -> Release {
        Release {
            package: "pkg".into(),
            old_version: Version::new(1, 0, 0),
            version: Version::new(1, 1, 0),
            bump: Bump::Minor,
            tag: "v1.1.0".into(),
            notes: vec!["Fix\n\n```rs\nrun();\n```".into(), "Feature".into()],
            note_bumps: vec![Bump::Patch, Bump::Minor],
        }
    }

    #[test]
    fn preserves_prefix_and_prior_versions() {
        let entry = "## 1.1.0\n\n### Minor Changes\n\n- Feature\n\n### Patch Changes\n\n- Fix\n  \n  ```rs\n  run();\n  ```\n\n";
        for (old, prefix, suffix) in [
            ("", "", ""),
            ("## 1.0.0\n\nOld\n", "", "## 1.0.0\n\nOld\n"),
            (
                "# Changelog\n\nIntro\n\n## 1.0.0\nOld\n",
                "# Changelog\n\nIntro\n\n",
                "## 1.0.0\nOld\n",
            ),
            ("# Changelog", "# Changelog\n\n", ""),
            ("Intro\n", "Intro\n\n", ""),
            (
                "```md\n## 9.0.0\n```\n\n## 1.0.0\n",
                "```md\n## 9.0.0\n```\n\n",
                "## 1.0.0\n",
            ),
            (
                "~~~\n## 9.0.0\n~~~\n\n## 1.0.0\n",
                "~~~\n## 9.0.0\n~~~\n\n",
                "## 1.0.0\n",
            ),
            (
                "    ## 9.0.0\n\n## 1.0.0\n",
                "    ## 9.0.0\n\n",
                "## 1.0.0\n",
            ),
            (
                "# Changelog\r\n\r\n## 1.0.0\r\n",
                "# Changelog\r\n\r\n",
                "## 1.0.0\r\n",
            ),
        ] {
            assert_eq!(
                render(&release(), old, false),
                format!("{prefix}{entry}{suffix}")
            );
        }
    }
}
