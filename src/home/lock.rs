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
