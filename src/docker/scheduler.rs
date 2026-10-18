//! Lightweight Docker operation scheduler for heavy workflows.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const SLOT_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const HEAVY_SLOT_WAIT_ATTEMPTS: usize = 2_400;
const BUILD_SLOT_WAIT_ATTEMPTS: usize = 7_200;

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
    let lock_root = lock_root(class);
    std::fs::create_dir_all(&lock_root)
        .with_context(|| format!("failed to create {}", lock_root.display()))?;
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

fn lock_root(class: DockerOpClass) -> PathBuf {
    let class_name = match class {
        DockerOpClass::Heavy => "heavy",
        DockerOpClass::Build => "build",
    };
    std::env::temp_dir()
        .join("berth-docker-scheduler")
        .join(class_name)
}

struct SlotGuard {
    path: PathBuf,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

fn acquire_slot(
    root: &Path,
    limit: usize,
    op_name: &str,
    class: DockerOpClass,
) -> Result<SlotGuard> {
    let max_attempts = match class {
        DockerOpClass::Heavy => HEAVY_SLOT_WAIT_ATTEMPTS,
        DockerOpClass::Build => BUILD_SLOT_WAIT_ATTEMPTS,
    };
    for _ in 0..max_attempts {
        for slot in 0..limit {
            let slot_path = root.join(format!("slot-{slot}.lock"));
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&slot_path)
            {
                Ok(mut file) => {
                    if writeln!(file, "{}:{op_name}", std::process::id()).is_err() {
                        // Best-effort metadata write; lock acquisition is what matters.
                    }
                    return Ok(SlotGuard { path: slot_path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    if should_reclaim_existing_slot(&slot_path) {
                        drop(std::fs::remove_file(&slot_path));
                    }
                    continue;
                }
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("failed to acquire {}", slot_path.display()));
                }
            }
        }
        std::thread::sleep(SLOT_WAIT_POLL_INTERVAL);
    }

    let wait_secs = SLOT_WAIT_POLL_INTERVAL.as_secs_f64() * max_attempts as f64;
    anyhow::bail!(
        "timed out waiting for runtime operation slot for '{op_name}' after {} attempts (~{wait_secs:.1}s)",
        max_attempts,
    )
}

fn should_reclaim_existing_slot(slot_path: &Path) -> bool {
    should_reclaim_existing_slot_with(slot_path, process_exists)
}

fn should_reclaim_existing_slot_with<F>(slot_path: &Path, process_exists: F) -> bool
where
    F: Fn(u32) -> bool,
{
    let Some(pid) = read_slot_pid(slot_path) else {
        return false;
    };

    !process_exists(pid)
}

fn read_slot_pid(slot_path: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(slot_path).ok()?;
    raw.trim()
        .split_once(':')
        .map_or(raw.trim(), |(pid, _)| pid)
        .parse::<u32>()
        .ok()
}

fn process_exists(pid: u32) -> bool {
    process_exists_with_command("kill", pid)
}

fn process_exists_with_command(command: &str, pid: u32) -> bool {
    Command::new(command)
        .args(["-0", &pid.to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::{
        BUILD_SLOT_WAIT_ATTEMPTS, DockerOpClass, HEAVY_SLOT_WAIT_ATTEMPTS, SLOT_WAIT_POLL_INTERVAL,
        process_exists_with_command, read_slot_pid, should_reclaim_existing_slot_with,
        with_scheduled_docker_op,
    };
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    static PATH_MUTEX: Mutex<()> = Mutex::new(());

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-docker-scheduler-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
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
    fn build_slot_timeout_budget_exceeds_long_image_builds() {
        let wait_budget = SLOT_WAIT_POLL_INTERVAL.saturating_mul(BUILD_SLOT_WAIT_ATTEMPTS as u32);
        assert!(
            wait_budget >= Duration::from_secs(60),
            "build slot wait budget should cover long derived image builds"
        );
    }

    #[test]
    fn heavy_slot_timeout_budget_covers_slow_runtime_cleanup() {
        let wait_budget = SLOT_WAIT_POLL_INTERVAL.saturating_mul(HEAVY_SLOT_WAIT_ATTEMPTS as u32);
        assert!(
            wait_budget >= Duration::from_secs(60),
            "heavy slot wait budget should cover slow runtime cleanup"
        );
    }

    #[test]
    fn read_slot_pid_parses_pid_prefix_before_op_name() {
        let root = temp_root("read-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "424242:docker-volume-rm-runtime-test\n").expect("write slot");

        assert_eq!(read_slot_pid(&slot_path), Some(424242));
    }

    #[test]
    fn stale_slot_with_dead_pid_is_reclaimed() {
        let root = temp_root("stale-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "424242:docker-rm-runtime-test-container\n").expect("write slot");

        assert!(should_reclaim_existing_slot_with(&slot_path, |_| false));
    }

    #[test]
    fn slot_with_live_pid_is_not_reclaimed() {
        let root = temp_root("live-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "12345:docker-volume-rm-runtime-test\n").expect("write slot");

        assert!(!should_reclaim_existing_slot_with(&slot_path, |_| true));
    }

    #[test]
    fn process_exists_with_command_checks_pid_quietly() {
        let _guard = PATH_MUTEX.lock().unwrap_or_else(|err| err.into_inner());
        let root = temp_root("fake-kill");
        let binary = root.join("kill");
        let args_file = root.join("kill-args.txt");
        fs::write(
            &binary,
            format!(
                "#!/bin/sh\nprintf '%s %s' \"$1\" \"$2\" > '{}'\nprintf '%s' 'No such process' >&2\nexit 1\n",
                args_file.display()
            ),
        )
        .expect("write fake kill");
        let mut perms = fs::metadata(&binary).expect("kill metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&binary, perms).expect("chmod kill");

        assert!(!process_exists_with_command(
            binary.to_str().expect("binary path"),
            424242
        ));

        let mut recorded_args = String::new();
        fs::File::open(args_file)
            .expect("open args file")
            .read_to_string(&mut recorded_args)
            .expect("read args file");
        assert_eq!(recorded_args, "-0 424242");
    }
}
