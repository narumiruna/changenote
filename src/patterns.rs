//! Validated file and package-name patterns. Matching never uses lossy paths.
use anyhow::{Result, ensure};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer, de::Error};
use std::path::Path;

#[derive(Default)]
pub(crate) struct FilePatterns(Option<(GlobSet, GlobSet)>);

impl<'de> Deserialize<'de> for FilePatterns {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let patterns = Vec::<String>::deserialize(deserializer)?;
        Self::new(&patterns).map_err(D::Error::custom)
    }
}

impl FilePatterns {
    fn new(patterns: &[String]) -> Result<Self> {
        let mut included = GlobSetBuilder::new();
        let mut excluded = GlobSetBuilder::new();
        let mut has_inclusion = false;
        for value in patterns {
            let (exclude, pattern) = match value.strip_prefix('!') {
                Some(pattern) => (true, pattern),
                None => (false, value.as_str()),
            };
            ensure!(
                !(pattern.is_empty()
                    || pattern.starts_with('/')
                    || pattern.contains(['\\', '\0'])
                    || (pattern.as_bytes().get(1) == Some(&b':')
                        && pattern.as_bytes()[0].is_ascii_alphabetic())),
                "file patterns must be nonempty relative paths without backslashes"
            );
            for component in pattern.split('/') {
                ensure!(
                    !matches!(component, "" | "." | ".."),
                    "file patterns cannot contain empty, current, or parent components"
                );
                ensure!(
                    !component.contains("**") || component == "**",
                    "** must be a complete path component"
                );
            }
            let glob = GlobBuilder::new(pattern)
                .literal_separator(true)
                .backslash_escape(false)
                .empty_alternates(false)
                .allow_unclosed_class(false)
                .build()?;
            if exclude {
                excluded.add(glob);
            } else {
                has_inclusion = true;
                included.add(glob);
            }
        }
        ensure!(
            has_inclusion,
            "changed_file_patterns requires an inclusion pattern"
        );
        Ok(Self(Some((included.build()?, excluded.build()?))))
    }

    pub(crate) fn matches(&self, path: &Path) -> bool {
        self.0
            .as_ref()
            .is_none_or(|(included, excluded)| included.is_match(path) && !excluded.is_match(path))
    }
}

pub(crate) fn expand_fixed(groups: &[Vec<String>], names: &[&str]) -> Result<Vec<Vec<String>>> {
    groups
        .iter()
        .map(|group| {
            let mut result = Vec::new();
            for pattern in group {
                ensure!(
                    !pattern.is_empty()
                        && pattern
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"-_*?".contains(&c)),
                    "fixed patterns support only names, * and ?: {pattern}"
                );
                let mut normalized = String::new();
                for ch in pattern.chars() {
                    if ch != '*' || !normalized.ends_with('*') {
                        normalized.push(ch);
                    }
                }
                let matcher = GlobBuilder::new(&normalized)
                    .literal_separator(true)
                    .build()?
                    .compile_matcher();
                let matches: Vec<_> = names
                    .iter()
                    .filter(|name| matcher.is_match(name))
                    .map(|name| name.to_string())
                    .collect();
                ensure!(
                    !matches.is_empty(),
                    "unknown fixed package or unmatched pattern: {pattern}"
                );
                result.extend(matches);
            }
            result.sort();
            Ok(result)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(values: &[&str]) -> FilePatterns {
        FilePatterns::new(&values.iter().map(|s| s.to_string()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn path_contract_and_exclusion_precedence() {
        let p = patterns(&["**", "!docs/**", "docs/keep.rs"]);
        assert!(!p.matches(Path::new("docs/keep.rs")));
        assert!(p.matches(Path::new(".hidden")));
        let p = patterns(&["src/**/?[ab].{rs,py}"]);
        for path in ["src/xa.rs", "src/nested/yb.py", "src/.a.rs"] {
            assert!(p.matches(Path::new(path)), "{path}");
        }
        for path in ["src/xa.js", "lib/xa.rs", "src/abc.rs"] {
            assert!(!p.matches(Path::new(path)), "{path}");
        }
        assert!(!patterns(&["src/*"]).matches(Path::new("src/a/b.rs")));
        assert!(patterns(&["**/x"]).matches(Path::new("x")));
    }

    #[test]
    fn fixed_name_contract_is_case_sensitive_and_has_no_path_semantics() {
        let names = ["sdk-a", "sdk-b", "Sdk-c", "cli"];
        for pattern in ["sdk-*", "sdk-?", "sdk-***"] {
            assert_eq!(
                expand_fixed(&[vec![pattern.into()]], &names).unwrap()[0],
                ["sdk-a", "sdk-b"]
            );
        }
        for bad in [
            "sdk-[ab]",
            "{sdk-a,sdk-b}",
            "!sdk-*",
            "sdk/a",
            "sdk\\a",
            "@(sdk-a)",
            "missing*",
        ] {
            assert!(expand_fixed(&[vec![bad.into()]], &names).is_err());
        }
    }

    #[test]
    fn rejects_invalid_patterns() {
        for values in [
            vec![],
            vec![""],
            vec!["!"],
            vec!["!docs/**"],
            vec!["/src/**"],
            vec!["C:/src/**"],
            vec!["src\\x"],
            vec!["./src/**"],
            vec!["src/../x"],
            vec!["src//x"],
            vec!["src/"],
            vec!["src/**x"],
            vec!["["],
            vec!["{a,b"],
            vec!["[z-a]"],
        ] {
            assert!(
                FilePatterns::new(&values.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                    .is_err(),
                "{values:?}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_paths_are_matched_without_conversion() {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};
        let path = PathBuf::from(OsString::from_vec(b"src/\xff.rs".to_vec()));
        assert!(patterns(&["src/*.rs"]).matches(&path));
        assert!(FilePatterns::default().matches(&path));
        assert!(!patterns(&["**", "!src/**"]).matches(&path));
    }
}
