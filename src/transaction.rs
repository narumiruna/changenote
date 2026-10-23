//! Best-effort recovery for known release files after an ordinary error.
//!
//! This does not provide crash safety, atomic updates, or concurrent-writer safety.
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

struct Snapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Snapshot each distinct path before the operation. On error, try every restore.
/// Only the listed files are restored; directories and unrelated files are not removed.
/// Symlinks, including symlink parent directories, are not accepted or followed.
pub(crate) fn apply<F: FnOnce() -> Result<()>>(paths: &[PathBuf], operation: F) -> Result<()> {
    let mut seen = HashSet::new();
    let mut snapshots = Vec::new();
    for path in paths {
        let path = std::path::absolute(path)
            .with_context(|| format!("cannot resolve transaction path {}", path.display()))?;
        if seen.insert(path.clone()) {
            let bytes = if regular_file_exists(&path)? {
                Some(fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?)
            } else {
                None
            };
            snapshots.push(Snapshot { path, bytes });
        }
    }

    if let Err(error) = operation() {
        let mut failures = Vec::new();
        for snapshot in snapshots {
            if let Err(restore_error) = snapshot.restore() {
                failures.push(format!("{}: {restore_error:#}", snapshot.path.display()));
            }
        }
        if failures.is_empty() {
            return Err(error);
        }
        let diagnostic = format!(
            "operation failed ({error:#}); cannot restore transaction paths: {}",
            failures.join("; ")
        );
        return Err(error.context(diagnostic));
    }
    Ok(())
}

/// Check known output paths without writing files or taking byte snapshots.
pub(crate) fn validate_paths(paths: &[PathBuf]) -> Result<()> {
    for path in paths {
        let path = std::path::absolute(path)?;
        regular_file_exists(&path)?;
    }
    Ok(())
}

impl Snapshot {
    fn restore(&self) -> Result<()> {
        let exists = regular_file_exists(&self.path)?;
        match &self.bytes {
            Some(bytes) => fs::write(&self.path, bytes).context("cannot restore original bytes"),
            None if exists => fs::remove_file(&self.path).context("cannot remove new file"),
            None => Ok(()),
        }
    }
}

/// Check parents as well as the file, including when the file does not exist.
/// Repeat this check before restoration because the operation can replace paths.
fn regular_file_exists(path: &Path) -> Result<bool> {
    for parent in path.ancestors().skip(1) {
        match fs::symlink_metadata(parent) {
            Ok(metadata) => ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "transaction parents must be directories and cannot be symlinks: {}",
                parent.display()
            ),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot inspect parent {}", parent.display()));
            }
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "transaction paths must be regular files and cannot be symlinks: {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("cannot inspect {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::bail;
    use tempfile::tempdir;

    #[test]
    fn failure_after_each_stage_restores_bytes_and_absent_outputs() -> Result<()> {
        for failed_stage in 0..7 {
            let directory = tempdir()?;
            let root = directory.path();
            let paths: Vec<_> = [
                "Cargo.toml",
                "Cargo.lock",
                "CHANGELOG.md",
                "new-changelog.md",
                "releases.json",
                "note-a.md",
                "note-b.md",
            ]
            .iter()
            .map(|name| root.join(name))
            .collect();
            // Exercise both an existing batch and an absent batch.
            for existing_batch in [false, true] {
                let originals: Vec<Option<Vec<u8>>> = vec![
                    Some(b"manifest\r\n\0\xff".to_vec()),
                    Some(b"lockfile\n".to_vec()),
                    Some(Vec::new()),
                    None,
                    existing_batch.then(|| b"previous batch\n".to_vec()),
                    Some(b"note a\r\n".to_vec()),
                    Some(b"note b without final newline".to_vec()),
                ];
                for (path, bytes) in paths.iter().zip(&originals) {
                    match bytes {
                        Some(bytes) => fs::write(path, bytes)?,
                        None if path.exists() => fs::remove_file(path)?,
                        None => {}
                    }
                }
                let unrelated = root.join("unrelated.txt");
                let error = apply(&paths, || {
                    fs::write(&unrelated, "keep this new file")?;
                    for (stage, path) in paths.iter().enumerate() {
                        if stage < 5 {
                            fs::write(path, "updated")?;
                        } else {
                            fs::remove_file(path)?;
                        }
                        if stage == failed_stage {
                            bail!("injected failure after stage {stage}");
                        }
                    }
                    Ok(())
                })
                .unwrap_err();
                assert_eq!(
                    error.to_string(),
                    format!("injected failure after stage {failed_stage}")
                );
                for (path, bytes) in paths.iter().zip(&originals) {
                    match bytes {
                        Some(bytes) => assert_eq!(&fs::read(path)?, bytes),
                        None => assert!(!path.try_exists()?),
                    }
                }
                assert_eq!(fs::read(&unrelated)?, b"keep this new file");
            }
        }
        Ok(())
    }

    #[test]
    fn success_keeps_updates_new_outputs_and_consumed_notes() -> Result<()> {
        let directory = tempdir()?;
        let version = directory.path().join("VERSION");
        let output = directory.path().join("CHANGELOG.md");
        let note = directory.path().join("note.md");
        fs::write(&version, "1.0.0")?;
        fs::write(&note, "pending")?;
        apply(&[version.clone(), output.clone(), note.clone()], || {
            fs::write(&version, "1.0.1")?;
            fs::write(&output, "release")?;
            fs::remove_file(&note)?;
            Ok(())
        })?;
        assert_eq!(fs::read(version)?, b"1.0.1");
        assert_eq!(fs::read(output)?, b"release");
        assert!(!note.try_exists()?);
        Ok(())
    }

    #[test]
    fn invalid_output_prevents_operation() -> Result<()> {
        let directory = tempdir()?;
        let version = directory.path().join("VERSION");
        let invalid = directory.path().join("CHANGELOG.md");
        fs::write(&version, "original")?;
        fs::create_dir(&invalid)?;
        let mut called = false;
        let error = apply(&[version.clone(), invalid.clone()], || {
            called = true;
            fs::write(&version, "changed")?;
            Ok(())
        })
        .unwrap_err();
        assert!(!called);
        assert!(error.to_string().contains(&invalid.display().to_string()));
        assert_eq!(fs::read(version)?, b"original");
        Ok(())
    }

    #[test]
    fn invalid_parent_prevents_operation() -> Result<()> {
        let directory = tempdir()?;
        let parent = directory.path().join("not-a-directory");
        fs::write(&parent, "file")?;
        let mut called = false;
        assert!(
            apply(&[parent.join("output")], || {
                called = true;
                Ok(())
            })
            .is_err()
        );
        assert!(!called);
        Ok(())
    }

    #[test]
    fn all_restore_failures_are_reported_and_other_paths_are_restored() -> Result<()> {
        let directory = tempdir()?;
        let blocked = directory.path().join("blocked-existing");
        let restored = directory.path().join("restored-note");
        let blocked_new = directory.path().join("blocked-new");
        let removed = directory.path().join("new-output");
        fs::write(&blocked, "original")?;
        fs::write(&restored, "note bytes")?;
        let paths = [
            blocked.clone(),
            restored.clone(),
            blocked_new.clone(),
            removed.clone(),
            blocked.clone(), // A duplicate must not produce a second restore failure.
        ];
        let error = apply(&paths, || {
            fs::remove_file(&blocked)?;
            fs::create_dir(&blocked)?;
            fs::write(blocked.join("unrelated"), "keep")?;
            fs::remove_file(&restored)?;
            fs::create_dir(&blocked_new)?;
            fs::write(&removed, "new")?;
            Err(std::io::Error::other("original operation error").into())
        })
        .unwrap_err();
        let diagnostic = error.to_string();
        assert!(diagnostic.contains("original operation error"));
        assert!(diagnostic.contains("cannot restore transaction paths"));
        // Each failed path appears once as a label and once in its cause.
        assert_eq!(
            diagnostic.matches(&blocked.display().to_string()).count(),
            2
        );
        assert!(diagnostic.contains(&blocked_new.display().to_string()));
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert_eq!(fs::read(restored)?, b"note bytes");
        assert!(!removed.try_exists()?);
        assert_eq!(fs::read(blocked.join("unrelated"))?, b"keep");
        assert!(blocked_new.is_dir());
        Ok(())
    }

    #[test]
    fn empty_paths_still_run_operation_and_preserve_error() {
        let mut called = false;
        let error = apply(&[], || {
            called = true;
            bail!("operation error");
        })
        .unwrap_err();
        assert!(called);
        assert_eq!(error.to_string(), "operation error");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outputs_and_parents_prevent_operation() -> Result<()> {
        use std::os::unix::fs::symlink;
        let directory = tempdir()?;
        let outside = tempdir()?;
        let target = outside.path().join("target");
        fs::write(&target, "outside bytes")?;
        let link = directory.path().join("link");
        let dangling = directory.path().join("dangling");
        let parent = directory.path().join("parent");
        symlink(&target, &link)?;
        symlink(outside.path().join("absent"), &dangling)?;
        symlink(outside.path(), &parent)?;
        for path in [link, dangling, parent.join("target"), parent.join("absent")] {
            let mut called = false;
            let error = apply(&[path], || {
                called = true;
                Ok(())
            })
            .unwrap_err();
            assert!(!called);
            assert!(error.to_string().contains("symlink"));
        }
        assert_eq!(fs::read(target)?, b"outside bytes");
        assert!(!outside.path().join("absent").try_exists()?);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn restore_does_not_follow_replacement_symlinks() -> Result<()> {
        use std::os::unix::fs::symlink;
        for replace_parent in [false, true] {
            for originally_present in [false, true] {
                let directory = tempdir()?;
                let outside = tempdir()?;
                let parent = directory.path().join("parent");
                fs::create_dir(&parent)?;
                let path = parent.join("output");
                let target = outside.path().join("output");
                fs::write(&target, "outside bytes")?;
                if originally_present {
                    fs::write(&path, "original")?;
                }
                let other = directory.path().join("other");
                fs::write(&other, "other original")?;
                let error = apply(&[path.clone(), other.clone()], || {
                    fs::write(&other, "changed")?;
                    if originally_present {
                        fs::remove_file(&path)?;
                    }
                    if replace_parent {
                        fs::remove_dir(&parent)?;
                        symlink(outside.path(), &parent)?;
                    } else {
                        symlink(&target, &path)?;
                    }
                    bail!("operation error");
                })
                .unwrap_err();
                assert!(error.to_string().contains(&path.display().to_string()));
                assert!(error.to_string().contains("symlink"));
                assert_eq!(fs::read(target)?, b"outside bytes");
                assert_eq!(fs::read(other)?, b"other original");
            }
        }
        Ok(())
    }
}
