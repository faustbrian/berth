//! Fail-early check that the paths a PHP app writes in its checkout belong to
//! the user the container runs as (ledger L023).
//!
//! Containers of berth 7.0.0 ran as root, so on Linux a reused worktree can
//! hold root-owned logs, caches and dependencies. A container running as the
//! invoking user cannot write them: Laravel then answers 500 on every request
//! and Composer or npm fail. berth names the files and the fix instead.

use anyhow::Result;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Directories Laravel writes recursively at runtime.
const WRITABLE_TREES: [&str; 2] = ["storage", "bootstrap/cache"];
/// Dependency directories whose own entry must be writable (scanning them
/// whole would be slow; `composer install` and `npm install` replace their
/// contents).
const WRITABLE_DIRECTORIES: [&str; 2] = ["vendor", "node_modules"];
/// Bound on the entries scanned under each writable tree.
const MAX_SCANNED_ENTRIES: usize = 20_000;
/// Paths named in the error.
const MAX_REPORTED_PATHS: usize = 5;

/// Fails when a path the app writes is owned by another user.
///
/// # Errors
///
/// Returns an error naming the foreign-owned paths and the `chown` that
/// fixes them.
pub(super) fn ensure_checkout_writable_by(root: &Path, uid: u32, gid: u32) -> Result<()> {
    let foreign = foreign_owned_paths(root, uid);
    if foreign.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = foreign
        .iter()
        .take(MAX_REPORTED_PATHS)
        .map(|path| path.display().to_string())
        .collect();
    let more = foreign.len().saturating_sub(MAX_REPORTED_PATHS);
    anyhow::bail!(
        "{} path(s) in {} are not owned by UID {uid}, which the app container runs as (an older berth ran it as root): {}{}; fix them with `sudo chown -R {uid}:{gid} {}`",
        foreign.len(),
        root.display(),
        named.join(", "),
        if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        },
        WRITABLE_TREES
            .iter()
            .chain(WRITABLE_DIRECTORIES.iter())
            .filter(|relative| root.join(relative).exists())
            .copied()
            .collect::<Vec<_>>()
            .join(" ")
    )
}

/// Paths under the writable trees (bounded) and the dependency directories
/// themselves whose owner is not `uid`. Symlinks are judged by their own
/// entry and never followed.
fn foreign_owned_paths(root: &Path, uid: u32) -> Vec<PathBuf> {
    let mut foreign = Vec::new();
    for tree in WRITABLE_TREES {
        let mut pending = vec![root.join(tree)];
        let mut scanned = 0;
        while let Some(path) = pending.pop() {
            let Ok(metadata) = std::fs::symlink_metadata(&path) else {
                continue;
            };
            scanned += 1;
            if metadata.uid() != uid {
                foreign.push(path.clone());
            }
            if scanned >= MAX_SCANNED_ENTRIES {
                break;
            }
            if metadata.is_dir()
                && let Ok(entries) = std::fs::read_dir(&path)
            {
                pending.extend(entries.filter_map(Result::ok).map(|entry| entry.path()));
            }
        }
    }
    for directory in WRITABLE_DIRECTORIES {
        let path = root.join(directory);
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.uid() != uid
        {
            foreign.push(path);
        }
    }
    foreign.sort();
    foreign
}

#[cfg(test)]
mod tests {
    use super::{ensure_checkout_writable_by, foreign_owned_paths};
    use std::os::unix::fs::MetadataExt;

    #[test]
    fn paths_the_app_writes_must_belong_to_the_container_user() {
        let root = std::env::temp_dir().join(format!("berth-ownership-{}", std::process::id()));
        std::fs::create_dir_all(root.join("storage/logs")).expect("storage");
        std::fs::write(root.join("storage/logs/laravel.log"), b"").expect("log");
        std::fs::create_dir_all(root.join("vendor/acme")).expect("vendor");
        std::fs::create_dir_all(root.join("app")).expect("app");
        let owner = std::fs::metadata(&root).expect("root").uid();

        assert!(foreign_owned_paths(&root, owner).is_empty());
        ensure_checkout_writable_by(&root, owner, 0).expect("own checkout");

        // As another user every writable path is foreign; `app/` is not
        // checked and `vendor/acme` is not scanned.
        let other = owner + 1;
        let foreign = foreign_owned_paths(&root, other);
        assert_eq!(
            foreign,
            vec![
                root.join("storage"),
                root.join("storage/logs"),
                root.join("storage/logs/laravel.log"),
                root.join("vendor"),
            ]
        );
        let error = ensure_checkout_writable_by(&root, other, 7).expect_err("foreign");
        let message = format!("{error:#}");
        assert!(message.contains("4 path(s)"), "{message}");
        assert!(
            message.contains(&format!("sudo chown -R {other}:7 storage vendor")),
            "{message}"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
