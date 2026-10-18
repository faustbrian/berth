//! Removes test-slot lock files no test run holds.

use anyhow::{Context, Result};
use std::path::Path;

/// Deletes every `locks/test-slots/{workspace}/slot-{N}.lock` that no
/// running test holds and `occupied` (`{workspace}/slot-{N}.lock` of slots
/// that still have containers) does not name, while holding it, and returns
/// how many went. A run that opened one meanwhile notices its file is gone
/// and takes a new one (`same_file`). Called by the forced reap, under its
/// lock.
///
/// # Errors
///
/// Returns an error when a lock file cannot be inspected or removed.
pub(super) fn remove_free_test_slot_locks(
    occupied: &std::collections::BTreeSet<String>,
) -> Result<usize> {
    let root = crate::home::home_path(&Path::new("locks").join("test-slots"))?;
    let workspaces = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", root.display()));
        }
    };
    let mut removed = 0;
    for workspace in workspaces {
        let workspace = workspace.with_context(|| format!("failed to read {}", root.display()))?;
        if !workspace.file_type()?.is_dir() {
            continue;
        }
        for slot in std::fs::read_dir(workspace.path())
            .with_context(|| format!("failed to read {}", workspace.path().display()))?
        {
            let path = slot?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let is_slot = name.starts_with("slot-") && name.ends_with(".lock");
            let key = format!("{}/{name}", workspace.file_name().to_string_lossy());
            if is_slot && !occupied.contains(&key) && remove_if_free(&path)? {
                removed += 1;
            }
        }
    }
    Ok(removed)
}

fn remove_if_free(path: &Path) -> Result<bool> {
    let file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to open {}", path.display()));
        }
    };
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(false),
        Err(std::fs::TryLockError::Error(error)) => {
            return Err(error).with_context(|| format!("failed to lock {}", path.display()));
        }
    }
    if !crate::home::same_file(&file, path)? {
        return Ok(false);
    }
    std::fs::remove_file(path).with_context(|| format!("failed to delete {}", path.display()))?;
    Ok(true)
}
