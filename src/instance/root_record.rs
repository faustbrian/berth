//! The checkout an instance's berth state belongs to, recorded beside that
//! state when berth first writes it (generated credentials or an APP_KEY).
//!
//! An instance that never ran `up` has no lease and no containers, so its
//! state directory would otherwise name no checkout: it could never be
//! reaped when the checkout is deleted, and `ls` would list it forever.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "root";

/// Records `root` for the instance unless a record exists.
///
/// # Errors
///
/// Returns an error when the record cannot be written.
pub(crate) fn record_root(project: &str, instance: &str, root: &Path) -> Result<()> {
    if cfg!(test) || crate::docker::is_dry_run() {
        return Ok(());
    }
    let dir = crate::home::instance_dir(project, instance)?;
    let path = dir.join(FILE_NAME);
    if path.exists() {
        return Ok(());
    }
    let staged = dir.join(format!(".{FILE_NAME}.{}", std::process::id()));
    std::fs::write(&staged, root.as_os_str().as_encoded_bytes())
        .with_context(|| format!("failed to write {}", staged.display()))?;
    std::fs::rename(&staged, &path).with_context(|| format!("failed to write {}", path.display()))
}

/// The recorded checkout of an instance, if any.
pub(crate) fn recorded_root(project: &str, instance: &str) -> Option<PathBuf> {
    let path = crate::home::home_path(
        &Path::new("instances")
            .join(project)
            .join(instance)
            .join(FILE_NAME),
    )
    .ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}
