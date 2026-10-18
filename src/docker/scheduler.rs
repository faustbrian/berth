//! Lightweight Docker operation scheduler for heavy workflows.

use anyhow::{Context, Result};
use std::fs::TryLockError;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SLOT_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DockerOpClass {
    Heavy,
    Build,
}

pub(crate) fn with_scheduled_docker_op<T, F>(
    class: DockerOpClass,
    op_name: &str,
    operation: F,
) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    if crate::docker::is_dry_run() {
        return operation();
    }

    let limit = concurrency_limit(class).max(1);
    let lock_root = lock_root(class)?;
    let _slot = acquire_slot(&lock_root, limit, op_name, class)?;
    operation()
}

fn concurrency_limit(class: DockerOpClass) -> usize {
    let policy = super::policy::docker_policy();
    match class {
        DockerOpClass::Heavy => policy.max_heavy_ops,
        DockerOpClass::Build => policy.max_build_ops.min(policy.max_heavy_ops),
    }
}

fn class_name(class: DockerOpClass) -> &'static str {
    match class {
        DockerOpClass::Heavy => "heavy",
        DockerOpClass::Build => "build",
    }
}

/// The slots live in the berth home (private to its user), not the shared
/// temp directory, where another user's files could block or deny them.
fn lock_root(class: DockerOpClass) -> Result<PathBuf> {
    #[cfg(test)]
    {
        let root = crate::test_support::temp_root()
            .join("engine-slots")
            .join(class_name(class));
        std::fs::create_dir_all(&root)
            .with_context(|| format!("failed to create {}", root.display()))?;
        Ok(root)
    }
    #[cfg(not(test))]
    crate::home::private_dir(&Path::new("locks").join(format!("engine-{}", class_name(class))))
}

/// One held slot: an exclusive `flock` on `slot-{n}.lock`, released when
/// dropped or when the process dies, so a crashed berth never keeps a slot.
struct SlotGuard {
    _file: std::fs::File,
}

/// Takes the first free slot of `limit`, waiting as long as all are busy:
/// how long other operations take depends on how loaded the engine is, and
/// a holder that dies frees its slot. Reports the wait on stderr.
fn acquire_slot(
    root: &Path,
    limit: usize,
    op_name: &str,
    class: DockerOpClass,
) -> Result<SlotGuard> {
    let mut notified = false;
    loop {
        for slot in 0..limit {
            let slot_path = root.join(format!("slot-{slot}.lock"));
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .mode(0o600)
                .open(&slot_path)
                .with_context(|| format!("failed to open {}", slot_path.display()))?;
            match file.try_lock() {
                Ok(()) => return Ok(SlotGuard { _file: file }),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", slot_path.display()));
                }
            }
        }
        crate::parallel::ensure_no_sibling_failed()?;
        if !notified {
            notified = true;
            crate::output::event(
                "engine",
                crate::output::LogLevel::Info,
                &format!(
                    "'{op_name}' waits for one of {limit} {} slots; other berth processes hold them",
                    class_name(class)
                ),
                crate::output::Persistence::Persistent,
            );
        }
        std::thread::sleep(SLOT_WAIT_POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::{DockerOpClass, acquire_slot, with_scheduled_docker_op};
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::docker::unique_test_path(
            crate::test_support::temp_root().join(format!("berth-docker-scheduler-{name}")),
        );
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn scheduled_op_runs_and_returns_result() {
        let value = with_scheduled_docker_op(DockerOpClass::Heavy, "test-op", || {
            Ok::<_, anyhow::Error>(7)
        })
        .expect("scheduled op result");
        assert_eq!(value, 7);
    }

    #[test]
    fn a_full_scheduler_waits_for_a_slot_instead_of_failing() {
        let root = temp_root("full");
        let held = acquire_slot(&root, 1, "holder", DockerOpClass::Build).expect("first slot");

        let (sender, receiver) = std::sync::mpsc::channel();
        let waiter_root = root.clone();
        std::thread::spawn(move || {
            sender
                .send(acquire_slot(&waiter_root, 1, "waiter", DockerOpClass::Build).map(|_| ()))
                .expect("send");
        });
        assert!(
            receiver.recv_timeout(Duration::from_millis(500)).is_err(),
            "every slot is held, so the waiter keeps waiting"
        );
        drop(held);
        receiver
            .recv_timeout(Duration::from_secs(10))
            .expect("the waiter gets the slot once it is released")
            .expect("acquired");
    }

    #[test]
    fn a_slot_file_a_crashed_berth_left_behind_is_taken_at_once() {
        let root = temp_root("crashed");
        // 9.x wrote `pid:op` into a slot file and deleted it on drop; a
        // crash left the file, and one without a pid was never reclaimed.
        fs::write(root.join("slot-0.lock"), "").expect("leftover slot file");
        let started = std::time::Instant::now();
        let _slot = acquire_slot(&root, 1, "after-crash", DockerOpClass::Heavy).expect("slot");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "no holder, no wait"
        );
    }

    #[test]
    fn a_free_slot_is_taken_while_another_is_held() {
        let root = temp_root("free");
        let _first = acquire_slot(&root, 2, "first", DockerOpClass::Heavy).expect("first slot");
        let _second = acquire_slot(&root, 2, "second", DockerOpClass::Heavy).expect("second slot");
    }
}
