//! Whether an open file is still the one at its path.

use anyhow::{Context, Result};
use std::path::Path;

/// Whether `file` is the file at `path` now. A lock taken on a file that
/// was deleted (and perhaps recreated) meanwhile guards nothing anyone else
/// can see.
///
/// # Errors
///
/// Returns an error when either cannot be inspected.
pub(crate) fn same_file(file: &std::fs::File, path: &Path) -> Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let held = file
        .metadata()
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    match std::fs::metadata(path) {
        Ok(current) => Ok(held.dev() == current.dev() && held.ino() == current.ino()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}
