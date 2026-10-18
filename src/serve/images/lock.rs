//! Derived-image cache lock persistence and existence checks.

use anyhow::{Context, Result};
use std::path::PathBuf;

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

/// An exclusive `flock` on the lock file, released when dropped or when the
/// process dies. The file stays: deleting it would let a waiter lock a file
/// no longer at the path.
struct FileLockGuard {
    _file: std::fs::File,
}

/// Blocks until no other process holds the lock: the kernel releases a dead
/// holder's lock, so there is nothing to time out on.
fn acquire_file_lock(path: &std::path::Path) -> Result<FileLockGuard> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .with_context(|| format!("failed to open lock file {}", path.display()))?;
    file.lock()
        .with_context(|| format!("failed to lock {}", path.display()))?;
    Ok(FileLockGuard { _file: file })
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
        crate::test_support::temp_root().join(format!(
            "berth-derived-lock-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }

    #[test]
    fn a_lock_file_a_crashed_berth_left_behind_is_taken_at_once() {
        let lock_path = temp_path("crashed").with_extension("lock");
        // 9.x's lock was the file's existence; a crash left it behind and
        // every later berth timed out on it.
        fs::write(&lock_path, "").expect("leftover lock file");
        let started = std::time::Instant::now();
        let _guard = acquire_file_lock(&lock_path).expect("acquired");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn file_lock_waits_for_its_holder() {
        let lock_path = temp_path("guard").with_extension("lock");
        let held = acquire_file_lock(&lock_path).expect("acquire lock");

        let (sender, receiver) = std::sync::mpsc::channel();
        let waiter_path = lock_path.clone();
        std::thread::spawn(move || {
            sender
                .send(acquire_file_lock(&waiter_path).map(|_| ()))
                .expect("send");
        });
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(500))
                .is_err(),
            "a held lock keeps the waiter waiting"
        );
        drop(held);
        receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the waiter gets the lock once it is released")
            .expect("acquired");
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
