//! Locations inside the berth home directory.

use anyhow::{Context, Result};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};

/// Returns the berth home directory: `BERTH_HOME` when set, else `~/.berth`.
///
/// # Errors
///
/// Returns an error when neither `BERTH_HOME` nor `HOME` is set.
fn berth_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("BERTH_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    let home = std::env::var_os("HOME").context("HOME is not set and BERTH_HOME is not set")?;
    Ok(PathBuf::from(home).join(".berth"))
}

/// Returns a path below the berth home without creating anything, for reads
/// that must not create state (dry runs, config loads).
///
/// # Errors
///
/// Returns an error when neither `BERTH_HOME` nor `HOME` is set.
pub(crate) fn home_path(relative: &Path) -> Result<PathBuf> {
    Ok(berth_home()?.join(relative))
}

/// Deletes `~/.berth/instances/{project}/{instance}` (managed credentials and
/// keys). A missing directory is not an error.
///
/// # Errors
///
/// Returns an error when the directory exists but cannot be removed, or is a
/// symbolic link.
pub(crate) fn remove_instance_state(project: &str, instance: &str) -> Result<()> {
    let dir = home_path(&Path::new("instances").join(project).join(instance))?;
    match std::fs::symlink_metadata(&dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            anyhow::bail!("{} must not be a symbolic link", dir.display())
        }
        Ok(_) => std::fs::remove_dir_all(&dir)
            .with_context(|| format!("failed to delete {}", dir.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", dir.display())),
    }
}

/// Creates (if needed) and returns a user-private directory below the berth
/// home. The home itself and every directory below it must be a real
/// directory: a symbolic link anywhere from the home down is refused, so
/// berth never writes credentials or CA keys through a planted link.
///
/// # Errors
///
/// Returns an error when `relative` is not a plain relative path, or a
/// directory cannot be created, is a symbolic link, or is not a directory.
pub(crate) fn private_dir(relative: &Path) -> Result<PathBuf> {
    let home = berth_home()?;
    if let Some(parent) = home
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::DirBuilder::new()
            .recursive(true)
            .create(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut dir = home;
    ensure_private_component(&dir)?;
    for component in relative.components() {
        let Component::Normal(name) = component else {
            anyhow::bail!(
                "berth home path {} must be plain and relative",
                relative.display()
            );
        };
        dir.push(name);
        ensure_private_component(&dir)?;
    }
    Ok(dir)
}

fn ensure_private_component(dir: &Path) -> Result<()> {
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => {
            return Err(error).with_context(|| format!("failed to create {}", dir.display()));
        }
    }
    let metadata = std::fs::symlink_metadata(dir)
        .with_context(|| format!("failed to inspect {}", dir.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!(
            "{} must be a real directory, not a symbolic link",
            dir.display()
        );
    }
    if metadata.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to restrict {}", dir.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_private_component;

    #[test]
    fn symlinked_directories_are_refused() {
        let base = std::env::temp_dir().join(format!("berth-home-link-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&base));
        std::fs::create_dir_all(base.join("target")).expect("target");
        std::os::unix::fs::symlink(base.join("target"), base.join("home")).expect("link");
        let error = ensure_private_component(&base.join("home")).expect_err("symlink");
        assert!(error.to_string().contains("symbolic link"), "{error}");
        ensure_private_component(&base.join("real")).expect("real dir");
        std::fs::remove_dir_all(&base).expect("cleanup");
    }
}
