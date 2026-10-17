//! Exclusive host locks shared by concurrent berth processes.

use anyhow::{Context, Result};
use std::fs::{File, TryLockError};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::time::{Duration, Instant};

/// An exclusive advisory lock on a file under the berth home, held until
/// dropped. The kernel releases it when the process dies, so a killed berth
/// never leaves a stale lock behind.
#[derive(Debug)]
pub(crate) struct HostLock {
    _file: File,
}

impl HostLock {
    /// Acquires `locks/{name}.lock` without waiting: `None` when another
    /// process holds it.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock file cannot be opened or locked.
    pub(crate) fn try_acquire(name: &str) -> Result<Option<Self>> {
        let dir = super::private_dir(Path::new("locks"))?;
        let path = dir.join(format!("{name}.lock"));
        let file = open_lock_file(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => {
                Err(error).with_context(|| format!("failed to lock {}", path.display()))
            }
        }
    }

    /// Acquires `locks/{name}.lock`, waiting at most `timeout`.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock file cannot be opened or the lock is
    /// still held by another process after `timeout`.
    pub(crate) fn acquire(name: &str, timeout: Duration) -> Result<Self> {
        let dir = super::private_dir(Path::new("locks"))?;
        let path = dir.join(format!("{name}.lock"));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        let started = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(TryLockError::WouldBlock) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(TryLockError::WouldBlock) => anyhow::bail!(
                    "timed out after {}s waiting for {}; another berth process holds it",
                    timeout.as_secs(),
                    path.display()
                ),
                Err(TryLockError::Error(error)) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", path.display()));
                }
            }
        }
    }
}

fn open_lock_file(path: &Path) -> Result<File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))
}

/// Takes the lock on the existing file `path` without waiting: `None`
/// while another process holds it. The lock lasts until the file drops.
///
/// # Errors
///
/// Returns an error when the file cannot be opened or locked.
pub(crate) fn try_hold_lock(path: &Path) -> Result<Option<File>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("failed to lock {}", path.display()))
        }
    }
}
