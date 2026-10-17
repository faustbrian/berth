//! Cross-process serialization for Caddy config apply operations.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

const CADDY_APPLY_LOCK_ATTEMPTS: usize = 7_200;
const CADDY_APPLY_LOCK_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn with_caddy_apply_lock<R>(
    caddy_dir: &Path,
    operation: impl FnOnce() -> Result<R>,
) -> Result<R> {
    std::fs::create_dir_all(caddy_dir)
        .with_context(|| format!("failed to create {}", caddy_dir.display()))?;
    let _guard = acquire_caddy_apply_lock(caddy_dir)?;
    operation()
}

fn acquire_caddy_apply_lock(caddy_dir: &Path) -> Result<CaddyApplyLockGuard> {
    let lock_path = caddy_apply_lock_path(caddy_dir);
    for _ in 0..CADDY_APPLY_LOCK_ATTEMPTS {
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&lock_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                drop(writeln!(file, "{}", std::process::id()));
                return Ok(CaddyApplyLockGuard { path: lock_path });
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                reclaim_stale_lock_if_needed(&lock_path);
                std::thread::sleep(CADDY_APPLY_LOCK_INTERVAL);
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("failed to create {}", lock_path.display()));
            }
        }
    }

    anyhow::bail!(
        "timed out waiting for caddy apply lock {} after {} attempts",
        lock_path.display(),
        CADDY_APPLY_LOCK_ATTEMPTS
    )
}

fn caddy_apply_lock_path(caddy_dir: &Path) -> PathBuf {
    caddy_dir.join("apply.lock")
}

fn reclaim_stale_lock_if_needed(lock_path: &Path) {
    let Ok(content) = std::fs::read_to_string(lock_path) else {
        return;
    };

    let holder_pid = content.trim().parse::<u32>().ok();
    let stale = holder_pid.map(|pid| !process_exists(pid)).unwrap_or(true);

    if stale {
        drop(std::fs::remove_file(lock_path));
    }
}

#[cfg(unix)]
fn process_exists(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .output()
        .map(|output| {
            output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty()
        })
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn process_exists(_pid: u32) -> bool {
    true
}

struct CaddyApplyLockGuard {
    path: PathBuf,
}

impl Drop for CaddyApplyLockGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CADDY_APPLY_LOCK_ATTEMPTS, CADDY_APPLY_LOCK_INTERVAL, process_exists, with_caddy_apply_lock,
    };
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn temp_caddy_dir(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "berth-caddy-lock-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn with_caddy_apply_lock_serializes_concurrent_callers() {
        let caddy_dir = temp_caddy_dir("serialize");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        let entered_first = Arc::new(AtomicBool::new(false));
        let released_first = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();

        std::thread::scope(|scope| {
            let first_entered = entered_first.clone();
            let first_released = released_first.clone();
            let caddy_dir_first = caddy_dir.clone();
            scope.spawn(move || {
                with_caddy_apply_lock(&caddy_dir_first, || {
                    first_entered.store(true, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(150));
                    first_released.store(true, Ordering::SeqCst);
                    Ok::<_, anyhow::Error>(())
                })
                .expect("first lock");
            });

            while !entered_first.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }

            let first_released = released_first.clone();
            let caddy_dir_second = caddy_dir.clone();
            let tx_second = tx.clone();
            scope.spawn(move || {
                with_caddy_apply_lock(&caddy_dir_second, || {
                    tx_second
                        .send(first_released.load(Ordering::SeqCst))
                        .expect("send result");
                    Ok::<_, anyhow::Error>(())
                })
                .expect("second lock");
            });
        });

        assert_eq!(rx.recv().expect("receive result"), true);
        drop(std::fs::remove_dir_all(&caddy_dir));
    }

    #[test]
    fn with_caddy_apply_lock_reclaims_stale_empty_lock_file() {
        let caddy_dir = temp_caddy_dir("stale-empty");
        std::fs::create_dir_all(&caddy_dir).expect("create caddy dir");
        std::fs::write(caddy_dir.join("apply.lock"), "").expect("seed stale lock");

        with_caddy_apply_lock(&caddy_dir, || Ok::<_, anyhow::Error>(()))
            .expect("stale lock should be reclaimed");

        assert!(!caddy_dir.join("apply.lock").exists());
        drop(std::fs::remove_dir_all(&caddy_dir));
    }

    #[test]
    fn caddy_apply_lock_budget_covers_real_queueing() {
        let wait_budget =
            CADDY_APPLY_LOCK_INTERVAL.saturating_mul(CADDY_APPLY_LOCK_ATTEMPTS as u32);
        assert!(
            wait_budget >= Duration::from_secs(60),
            "caddy apply lock wait budget should cover normal queued updates"
        );
    }

    #[test]
    fn process_exists_returns_false_for_invalid_pid() {
        assert!(!process_exists(u32::MAX));
    }
}
