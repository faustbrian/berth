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
        loop {
            let file = open_lock_file(&path)?;
            match file.try_lock() {
                // `berth gc` deletes unused lock files while holding them; a
                // lock on a deleted file guards nothing, so take the new one.
                Ok(()) if super::same_file(&file, &path)? => return Ok(Some(Self { _file: file })),
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => return Ok(None),
                Err(TryLockError::Error(error)) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", path.display()));
                }
            }
        }
    }

    /// Acquires `locks/{name}.lock`, waiting as long as another process
    /// holds it. There is no deadline: the kernel releases the lock when its
    /// holder exits, so a wait ends unless a live berth keeps working, and how
    /// long that takes depends on how loaded the machine is. While waiting it
    /// reports so on stderr, then again every 30 s.
    ///
    /// # Errors
    ///
    /// Returns an error when the lock file cannot be opened or locked.
    pub(crate) fn acquire(name: &str) -> Result<Self> {
        let dir = super::private_dir(Path::new("locks"))?;
        Self::acquire_path(name, &dir.join(format!("{name}.lock")))
    }

    fn acquire_path(name: &str, path: &Path) -> Result<Self> {
        let mut notice = WaitNotice::new(name);
        loop {
            let file = open_lock_file(path)?;
            loop {
                match file.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => {
                        crate::parallel::ensure_no_sibling_failed()?;
                        notice.waiting();
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(TryLockError::Error(error)) => {
                        return Err(error)
                            .with_context(|| format!("failed to lock {}", path.display()));
                    }
                }
            }
            // As in `try_acquire`: only the file at the path now counts.
            if super::same_file(&file, path)? {
                return Ok(Self { _file: file });
            }
        }
    }
}

/// Reports a contended lock once, then every [`WaitNotice::REPEAT`].
struct WaitNotice<'a> {
    name: &'a str,
    started: Option<Instant>,
    last: Option<Instant>,
}

impl<'a> WaitNotice<'a> {
    const REPEAT: Duration = Duration::from_secs(30);

    fn new(name: &'a str) -> Self {
        Self {
            name,
            started: None,
            last: None,
        }
    }

    fn waiting(&mut self) {
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < Self::REPEAT)
        {
            return;
        }
        self.last = Some(now);
        let message = if now == started {
            format!(
                "waiting for the {} lock, which another berth process holds",
                self.name
            )
        } else {
            format!(
                "still waiting for the {} lock ({}s)",
                self.name,
                now.duration_since(started).as_secs()
            )
        };
        crate::output::event(
            "lock",
            crate::output::LogLevel::Info,
            &message,
            crate::output::Persistence::Persistent,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn lock_path(name: &str) -> std::path::PathBuf {
        let dir = crate::docker::unique_test_path(
            crate::test_support::temp_root().join(format!("host-lock-{name}")),
        );
        std::fs::create_dir_all(&dir).expect("lock dir");
        dir.join(format!("{name}.lock"))
    }

    #[test]
    fn acquire_waits_for_the_holder_instead_of_failing() {
        let path = lock_path("wait");
        let held = open_lock_file(&path).expect("open");
        held.lock().expect("hold");

        let (sender, receiver) = mpsc::channel();
        let waiter_path = path.clone();
        std::thread::spawn(move || {
            sender
                .send(HostLock::acquire_path("wait", &waiter_path))
                .expect("send");
        });

        assert!(
            receiver.recv_timeout(Duration::from_millis(500)).is_err(),
            "a held lock keeps the waiter waiting, it never fails it"
        );
        drop(held);
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the waiter gets the lock once it is released")
            .expect("acquired");
    }

    #[test]
    fn acquire_holds_the_file_now_at_the_path_after_gc_deleted_it() {
        let path = lock_path("gc");
        let held = open_lock_file(&path).expect("open");
        held.lock().expect("hold");

        let (sender, receiver) = mpsc::channel();
        let waiter_path = path.clone();
        std::thread::spawn(move || {
            sender
                .send(HostLock::acquire_path("gc", &waiter_path))
                .expect("send");
        });
        std::thread::sleep(Duration::from_millis(200));
        // `berth gc` deletes a lock file while holding it.
        std::fs::remove_file(&path).expect("delete held lock file");
        drop(held);

        let _waiter = receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("waiter finished")
            .expect("acquired");
        let fresh = open_lock_file(&path).expect("open the file at the path");
        assert!(
            matches!(fresh.try_lock(), Err(TryLockError::WouldBlock)),
            "the waiter holds the lock file that is at the path now"
        );
    }
}
