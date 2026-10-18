//! Removes instance lock files of instances that no longer exist.

use anyhow::{Context, Result};
use std::collections::BTreeSet;

/// Deletes `locks/instance_{project}_{instance}.lock` and
/// `locks/lease_{project}_{instance}.lock` for every instance not in
/// `known` (those with state or containers), while holding each one
/// exclusively, and returns how many went. Every berth command in a project
/// marks its instance in use before loading it, so a checkout where only
/// `env` or `config validate` ever ran leaves these behind. A command that
/// opened one meanwhile notices it is gone and takes a new one
/// (`same_file`). Called by the forced reap, under its lock.
///
/// # Errors
///
/// Returns an error when the locks directory or a lock file cannot be read
/// or removed.
pub(super) fn remove_orphan_instance_locks(known: &BTreeSet<(String, String)>) -> Result<usize> {
    let dir = crate::home::home_path(std::path::Path::new("locks"))?;
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", dir.display()));
        }
    };
    let mut removed = 0;
    for entry in entries {
        let path = entry
            .with_context(|| format!("failed to read {}", dir.display()))?
            .path();
        let Some(identity) = path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(instance_of)
        else {
            continue;
        };
        if known.contains(&identity) {
            continue;
        }
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("failed to open {}", path.display()));
            }
        };
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => continue,
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(error).with_context(|| format!("failed to lock {}", path.display()));
            }
        }
        if crate::home::same_file(&file, &path)? {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
            removed += 1;
        }
    }
    Ok(removed)
}

/// The project and instance of an `instance_` or `lease_` lock file name;
/// `_` occurs in neither part.
fn instance_of(name: &str) -> Option<(String, String)> {
    let stem = name.strip_suffix(".lock")?;
    let rest = stem
        .strip_prefix("instance_")
        .or_else(|| stem.strip_prefix("lease_"))?;
    let (project, instance) = rest.split_once('_')?;
    (!project.is_empty() && !instance.is_empty() && !instance.contains('_'))
        .then(|| (project.to_owned(), instance.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::instance_of;

    #[test]
    fn only_instance_and_lease_locks_name_an_instance() {
        assert_eq!(
            instance_of("instance_shop_a-1b2c3d.lock"),
            Some(("shop".to_owned(), "a-1b2c3d".to_owned()))
        );
        assert_eq!(
            instance_of("lease_shop_a-1b2c3d.lock"),
            Some(("shop".to_owned(), "a-1b2c3d".to_owned()))
        );
        for other in [
            "reap.lock",
            "gateway.lock",
            "instance_shop.lock",
            "network-allocation.lock",
        ] {
            assert_eq!(instance_of(other), None, "{other}");
        }
    }
}
