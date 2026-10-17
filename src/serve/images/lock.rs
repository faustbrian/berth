//! Derived-image cache lock persistence and existence checks.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::time::Duration;

use super::super::DerivedImageLock;

/// Returns the lockfile path used to map image signatures to derived tags.
fn derived_image_lock_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/berth/cache/derived-image-lock.toml"))
}

/// Reads derived image lock from persisted or external state.
pub(super) fn read_derived_image_lock() -> Result<DerivedImageLock> {
    let path = derived_image_lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let lock_path = lockfile_path(&path);
    let _guard = acquire_file_lock(&lock_path)?;
    if !path.exists() {
        return Ok(DerivedImageLock::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let parsed: DerivedImageLock =
        toml::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(parsed)
}

/// Writes derived image lock to persisted or external state.
pub(super) fn write_derived_image_lock(lock: &DerivedImageLock) -> Result<()> {
    let path = derived_image_lock_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let lock_path = lockfile_path(&path);
    let _guard = acquire_file_lock(&lock_path)?;
    let content = toml::to_string_pretty(lock).context("failed to serialize derived image lock")?;
    write_atomic_file(&path, &content)?;
    Ok(())
}

/// Returns whether a docker image tag currently exists locally.
pub(super) fn docker_image_exists(tag: &str) -> Result<bool> {
    if crate::docker::is_dry_run() {
        return Ok(false);
    }
    crate::docker::docker_image_exists(
        tag,
        &crate::docker::runtime_command_error_context("image inspect"),
    )
}

fn lockfile_path(path: &std::path::Path) -> PathBuf {
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .map(|ext| format!("{ext}.lock"))
        .unwrap_or_else(|| "lock".to_owned());
    path.with_extension(extension)
}

struct FileLockGuard {
    path: PathBuf,
}

impl Drop for FileLockGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

fn acquire_file_lock(path: &std::path::Path) -> Result<FileLockGuard> {
    let max_attempts = 200;
    for _ in 0..max_attempts {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
        {
            Ok(_) => {
                return Ok(FileLockGuard {
                    path: path.to_path_buf(),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to create lock file {}", path.display()));
            }
        }
    }

    anyhow::bail!(
        "timed out waiting for lock file {} after {} attempts",
        path.display(),
        max_attempts
    )
}

fn write_atomic_file(path: &std::path::Path, content: &str) -> Result<()> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("derived-image-lock");
    let tmp_name = format!("{file_name}.tmp-{unique}");
    let tmp_path = path.with_file_name(tmp_name);

    std::fs::write(&tmp_path, content)
        .with_context(|| format!("failed to write {}", tmp_path.display()))?;
    std::fs::rename(&tmp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{acquire_file_lock, write_atomic_file};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "berth-derived-lock-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn file_lock_guard_creates_and_cleans_lock_file() {
        let lock_path = temp_path("guard").with_extension("lock");
        assert!(!lock_path.exists());

        {
            let _guard = acquire_file_lock(&lock_path).expect("acquire lock");
            assert!(lock_path.exists());
        }

        assert!(!lock_path.exists());
    }

    #[test]
    fn write_atomic_file_replaces_content_without_temp_leak() {
        let target = temp_path("atomic").with_extension("toml");
        fs::write(&target, "old").expect("seed file");

        write_atomic_file(&target, "new-content").expect("atomic write");

        let content = fs::read_to_string(&target).expect("read target");
        assert_eq!(content, "new-content");
        let parent = target.parent().expect("parent");
        let has_tmp = fs::read_dir(parent)
            .expect("list parent")
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
            .any(|name| name.contains(".tmp-"));
        assert!(!has_tmp);
    }
}
