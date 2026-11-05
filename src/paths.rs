//! Name and path validation before repository file access.
use anyhow::{Result, ensure};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

pub(crate) fn safe_name(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c)),
        "name must contain only ASCII letters, digits, hyphens, or underscores: {name}"
    );
    Ok(())
}

pub(crate) fn relative(path: &Path) -> Result<()> {
    ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "path must be relative without parent components: {}",
        path.display()
    );
    Ok(())
}

// Resolve existing paths before writes so symlinks cannot escape the repository.
pub(crate) fn checked_path(root: &Path, path: &Path) -> Result<PathBuf> {
    use anyhow::Context;

    relative(path)?;
    let root = root.canonicalize()?;
    let full = root.join(path);
    let existing = match fs::symlink_metadata(&full) {
        Ok(_) => full.as_path(),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            full.parent().context("missing parent")?
        }
        Err(error) => return Err(error.into()),
    };
    ensure!(
        existing.canonicalize()?.starts_with(&root),
        "path escapes repository: {}",
        path.display()
    );
    Ok(full)
}

pub(crate) fn reject_output_symlinks(root: &Path, path: &Path) -> Result<()> {
    let root = root.canonicalize()?;
    let mut current = root.clone();
    for component in path.strip_prefix(&root)?.components() {
        current.push(component);
        ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "version output cannot contain a symlink: {}",
            current.display()
        );
    }
    Ok(())
}
