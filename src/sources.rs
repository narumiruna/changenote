//! Reproducible note-source links from local, first-parent Git history.
use anyhow::{Result, ensure};
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

pub(crate) fn validate_repository(repository: &str) -> Result<()> {
    let parts: Vec<_> = repository
        .strip_prefix("https://github.com/")
        .unwrap_or("")
        .split('/')
        .collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && !matches!(*part, "." | "..")
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            })
            && !parts[1].ends_with(".git"),
        "source_repository must be a canonical https://github.com/<owner>/<repo> URL"
    );
    Ok(())
}

fn git(root: &Path, args: &[&str], path: Option<&Path>) -> Option<Vec<u8>> {
    let mut command = Command::new("git");
    command
        .current_dir(root)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "--literal-pathspecs",
            "-c",
            "diff.renameLimit=0",
            "-c",
            "log.follow=false",
        ])
        .args(args);
    if let Some(path) = path {
        command.arg("--").arg(path);
    }
    let output = command.output().ok()?;
    output.status.success().then_some(output.stdout)
}

fn hashes(bytes: Vec<u8>) -> Option<Vec<String>> {
    let text = String::from_utf8(bytes).ok()?;
    text.lines()
        .map(|line| {
            ((line.len() == 40 || line.len() == 64) && line.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| line.to_owned())
        })
        .collect()
}

fn commits(root: &Path, path: &Path) -> Option<Vec<String>> {
    // A shallow boundary is not evidence of note creation. Never fetch history.
    if git(root, &["rev-parse", "--is-shallow-repository"], None)? != b"false\n" {
        return None;
    }
    let head = hashes(git(
        root,
        &["rev-parse", "--verify", "HEAD^{commit}"],
        None,
    )?)?
    .into_iter()
    .next()?;
    let tree = git(root, &["ls-tree", "-z", &head], Some(path))?;
    if !tree.starts_with(b"100644 blob ") && !tree.starts_with(b"100755 blob ") {
        return None;
    }
    // <revision>:./ resolves relative to the project, including nested projects.
    let mut object = OsString::from(format!("{head}:./"));
    object.push(path);
    let output = Command::new("git")
        .current_dir(root)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .arg("show")
        .arg(object)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Match the parser's CRLF normalization, without executing Git filters.
    let committed = String::from_utf8(output.stdout).ok()?.replace("\r\n", "\n");
    let current = fs::read_to_string(root.join(path))
        .ok()?
        .replace("\r\n", "\n");
    if committed != current {
        return None;
    }
    let mut top = git(root, &["rev-parse", "--show-toplevel"], None)?;
    top.pop().filter(|b| *b == b'\n')?;
    let top = std::path::PathBuf::from(filename(&top)?);
    let mut prefix = git(root, &["rev-parse", "--show-prefix"], None)?;
    prefix.pop().filter(|b| *b == b'\n')?;
    #[cfg(windows)]
    let name = path.to_str()?.replace('\\', "/");
    #[cfg(windows)]
    prefix.extend(name.as_bytes());
    #[cfg(not(windows))]
    prefix.extend(path.as_os_str().as_encoded_bytes());
    let mut path = std::path::PathBuf::from(filename(&prefix)?);
    let mut revision = head;
    let mut sources = std::collections::BTreeSet::new();
    loop {
        let history = |filter| {
            hashes(git(
                &top,
                &[
                    "log",
                    "--root",
                    "--first-parent",
                    "--no-renames",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--no-show-signature",
                    "--no-color",
                    "--format=%H",
                    filter,
                    &revision,
                ],
                Some(&path),
            )?)
        };
        // Do not use --follow: it also follows copies of identical files.
        let additions = history("--diff-filter=AT")?;
        let boundary = additions.first()?;
        let changes = history("--diff-filter=AMT")?;
        let end = changes.iter().position(|sha| sha == boundary)?;
        sources.extend(changes.into_iter().take(end + 1));
        let commit = git(&top, &["cat-file", "commit", boundary], None)?;
        let parent = commit
            .split(|b| *b == b'\n')
            .take_while(|line| !line.is_empty())
            .find_map(|line| line.strip_prefix(b"parent "));
        let Some(parent) = parent else {
            break;
        };
        let parent = String::from_utf8(parent.to_vec()).ok()?;
        let diff = git(
            &top,
            &[
                "diff-tree",
                "-r",
                "--no-commit-id",
                "--name-status",
                "-z",
                "--find-renames=50%",
                "--no-ext-diff",
                "--no-textconv",
                &parent,
                boundary,
            ],
            None,
        )?;
        match introduction(&diff, &path)? {
            Some(old) => {
                path = old;
                revision = parent;
            }
            None => break,
        }
    }
    Some(sources.into_iter().collect())
}

#[cfg(unix)]
fn filename(bytes: &[u8]) -> Option<OsString> {
    use std::os::unix::ffi::OsStringExt;
    Some(OsString::from_vec(bytes.to_vec()))
}

#[cfg(not(unix))]
fn filename(bytes: &[u8]) -> Option<OsString> {
    String::from_utf8(bytes.to_vec()).ok().map(Into::into)
}

// An addition starts a new lifecycle. Only an actual rename follows an old path.
fn introduction(diff: &[u8], path: &Path) -> Option<Option<std::path::PathBuf>> {
    let mut fields = diff.split(|b| *b == 0);
    while let Some(status) = fields.next().filter(|s| !s.is_empty()) {
        let from = std::path::PathBuf::from(filename(fields.next()?)?);
        if status.starts_with(b"R") || status.starts_with(b"C") {
            let to = std::path::PathBuf::from(filename(fields.next()?)?);
            if to == path {
                return Some(status.starts_with(b"R").then_some(from));
            }
        } else if matches!(status, b"A" | b"T") && from == path {
            return Some(None);
        }
    }
    None
}

pub(crate) fn render(
    root: &Path,
    path: &Path,
    repository: &str,
    summary: &str,
    required: bool,
) -> Result<String> {
    let Some(commits) = commits(root, path) else {
        ensure!(
            !required,
            "cannot prove note sources for {}; commit the note and provide complete Git history before versioning",
            path.display()
        );
        return Ok(summary.to_owned());
    };
    let links = commits
        .iter()
        .map(|sha| format!("[{}]({repository}/commit/{sha})", &sha[..12]))
        .collect::<Vec<_>>()
        .join(", ");
    ensure!(!links.is_empty(), "note sources cannot be empty");
    // Put sources before literal Markdown so even an unclosed fence cannot hide them.
    Ok(format!("Sources: {links}\n\n{summary}"))
}
