use crate::{Bump, note_content, write_note};
use anyhow::{Result, bail};
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
};

/// Create a note with an OS-random identifier; validation precedes allocation.
pub fn add_generated(root: &Path, packages: Vec<(String, Bump)>, message: &str) -> Result<PathBuf> {
    let content = note_content(root, packages, message)?;
    allocate(root, &content, || {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes)
            .map_err(|e| anyhow::anyhow!("note ID generation failed: {e}"))?;
        Ok(format!(
            "note-{}",
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ))
    })
}

fn allocate(
    root: &Path,
    content: &str,
    mut name: impl FnMut() -> Result<String>,
) -> Result<PathBuf> {
    for _ in 0..8 {
        match write_note(root, &name()?, content) {
            Ok(path) => return Ok(path),
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    bail!("could not allocate a unique note name after 8 attempts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn collision_success_exhaustion_and_other_errors() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".changenote")).unwrap();
        let taken = root.path().join(".changenote/taken.md");
        fs::write(&taken, "original").unwrap();
        let mut names = ["taken", "new"].into_iter();
        let path = allocate(root.path(), "content", || Ok(names.next().unwrap().into())).unwrap();
        assert_eq!(path.file_name().unwrap(), "new.md");
        assert_eq!(fs::read_to_string(&taken).unwrap(), "original");
        let mut calls = 0;
        assert!(
            allocate(root.path(), "content", || {
                calls += 1;
                Ok("taken".into())
            })
            .is_err()
        );
        assert_eq!(calls, 8);
        let missing = tempfile::tempdir().unwrap();
        calls = 0;
        assert!(
            allocate(missing.path(), "content", || {
                calls += 1;
                Ok("new".into())
            })
            .is_err()
        );
        assert_eq!(calls, 1);
        assert!(allocate(root.path(), "content", || bail!("randomness unavailable")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_collision_does_not_follow_target() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".changenote")).unwrap();
        let target = root.path().join("absent");
        std::os::unix::fs::symlink(&target, root.path().join(".changenote/taken.md")).unwrap();
        assert!(allocate(root.path(), "content", || Ok("taken".into())).is_err());
        assert!(!target.exists());
    }
}
