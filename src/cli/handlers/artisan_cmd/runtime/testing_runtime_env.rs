//! Unique testing runtime environment naming helpers.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static TEST_RUNTIME_POOL_SIZE_OVERRIDE: OnceLock<Mutex<Option<usize>>> = OnceLock::new();

/// Builds a unique runtime env namespace for one `berth artisan test` run.
pub(super) fn testing_runtime_env_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let run_id = format!("{:08x}", (nanos & 0xffff_ffff) as u32);

    format!("testing-{run_id}")
}

pub(crate) struct TestingRuntimeLease {
    runtime_env: String,
    _slot: Option<TestingRuntimeSlot>,
}

impl TestingRuntimeLease {
    pub(crate) fn runtime_env_name(&self) -> &str {
        &self.runtime_env
    }
}

struct TestingRuntimeSlot {
    path: std::path::PathBuf,
}

impl Drop for TestingRuntimeSlot {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

pub(super) fn acquire_testing_runtime_lease(workspace_root: &Path) -> Result<TestingRuntimeLease> {
    let pool_size = testing_runtime_pool_size();
    if pool_size == 0 {
        return Ok(TestingRuntimeLease {
            runtime_env: testing_runtime_env_name(),
            _slot: None,
        });
    }

    let workspace_key = workspace_pool_key(workspace_root);
    let lock_root = pool_lock_root(&workspace_key);
    std::fs::create_dir_all(&lock_root)
        .with_context(|| format!("failed to create {}", lock_root.display()))?;

    let max_attempts = 7_200;
    for _ in 0..max_attempts {
        for slot in 1..=pool_size {
            let slot_path = lock_root.join(format!("slot-{slot}.lock"));
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&slot_path)
            {
                Ok(mut file) => {
                    if writeln!(file, "{}", std::process::id()).is_err() {
                        // Best effort metadata write.
                    }
                    return Ok(TestingRuntimeLease {
                        runtime_env: resolve_pooled_runtime_env_name(&workspace_key, slot),
                        _slot: Some(TestingRuntimeSlot { path: slot_path }),
                    });
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
        std::thread::sleep(std::time::Duration::from_millis(25));
    }

    anyhow::bail!("timed out waiting for testing runtime slot after {max_attempts} attempts")
}

fn testing_runtime_pool_size() -> usize {
    let override_size = *TEST_RUNTIME_POOL_SIZE_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if let Some(value) = override_size {
        return value.max(1);
    }

    if let Some(value) = testing_runtime_pool_size_from_env(|name| {
        std::env::var_os(name).map(|value| value.to_string_lossy().to_string())
    }) {
        return value;
    }

    adaptive_testing_runtime_pool_size()
}

pub(crate) fn set_testing_runtime_pool_size_override(pool_size: Option<usize>) {
    let mut state = TEST_RUNTIME_POOL_SIZE_OVERRIDE
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *state = pool_size;
}

fn testing_runtime_pool_size_from_env<F>(lookup: F) -> Option<usize>
where
    F: Fn(&str) -> Option<String>,
{
    let raw = lookup("BERTH_TEST_RUNTIME_POOL_SIZE")?;
    raw.trim().parse::<usize>().ok().filter(|n| *n > 0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ResourceHint {
    cpus: usize,
    memory_bytes: u64,
}

fn adaptive_testing_runtime_pool_size() -> usize {
    let host_cpus = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .ok();
    let host_memory = host_memory_bytes();
    let docker_hint = docker_resource_hint();
    adaptive_pool_size_from_signals(host_cpus, host_memory, docker_hint)
}

fn adaptive_pool_size_from_signals(
    host_cpus: Option<usize>,
    host_memory_bytes: Option<u64>,
    docker_hint: Option<ResourceHint>,
) -> usize {
    let effective_cpus = docker_hint.map(|hint| hint.cpus).or(host_cpus).unwrap_or(2);
    let effective_memory = docker_hint
        .map(|hint| hint.memory_bytes)
        .or(host_memory_bytes)
        .unwrap_or(8_u64 * 1024 * 1024 * 1024);

    let cpu_bound = if effective_cpus >= 32 {
        32
    } else if effective_cpus >= 16 {
        16
    } else {
        8
    };

    let gib = effective_memory / (1024_u64 * 1024 * 1024);
    let mem_bound = if gib >= 64 {
        32
    } else if gib >= 24 {
        16
    } else {
        8
    };

    cpu_bound.min(mem_bound).clamp(8, 32)
}

fn docker_resource_hint() -> Option<ResourceHint> {
    let output = Command::new(crate::docker::docker_command())
        .args(["info", "--format", "{{.MemTotal}} {{.NCPU}}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut parts = stdout.split_whitespace();
    let memory_bytes = parts.next()?.trim().parse::<u64>().ok()?;
    let cpus = parts.next()?.trim().parse::<usize>().ok()?;
    if memory_bytes == 0 || cpus == 0 {
        return None;
    }

    Some(ResourceHint { cpus, memory_bytes })
}

#[cfg(target_os = "linux")]
fn host_memory_bytes() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = content.lines().find(|line| line.starts_with("MemTotal:"))?;
    let value_kib = line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())?;
    Some(value_kib.saturating_mul(1024))
}

#[cfg(target_os = "macos")]
fn host_memory_bytes() -> Option<u64> {
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn host_memory_bytes() -> Option<u64> {
    None
}

fn workspace_pool_key(workspace_root: &Path) -> String {
    let canonical = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let key = canonical.to_string_lossy();
    let hash = key.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(16777619).wrapping_add(u64::from(byte))
    });
    format!("{hash:016x}")
}

fn pool_lock_root(workspace_key: &str) -> std::path::PathBuf {
    std::env::temp_dir()
        .join("berth-testing-runtime-pool")
        .join(workspace_key)
}

fn resolve_pooled_runtime_env_name(workspace_key: &str, slot: usize) -> String {
    format!("testing-pool-{workspace_key}-{slot}")
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
    raw.trim().parse::<u32>().ok()
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
    use std::collections::HashMap;
    use std::fs;
    use std::io::Read;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ResourceHint, acquire_testing_runtime_lease, adaptive_pool_size_from_signals,
        process_exists_with_command, resolve_pooled_runtime_env_name,
        should_reclaim_existing_slot_with, testing_runtime_env_name,
        testing_runtime_pool_size_from_env, workspace_pool_key,
    };

    static PATH_MUTEX: Mutex<()> = Mutex::new(());

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "berth-test-runtime-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn runtime_env_name_uses_testing_prefix() {
        let runtime_env = testing_runtime_env_name();
        assert!(runtime_env.starts_with("testing-"));
    }

    #[test]
    fn runtime_env_name_has_non_empty_suffix() {
        let runtime_env = testing_runtime_env_name();
        let (_, suffix) = runtime_env
            .split_once('-')
            .expect("testing env should include separator");

        assert!(!suffix.is_empty());
    }

    #[test]
    fn pooled_runtime_name_is_stable_per_slot() {
        assert_eq!(
            resolve_pooled_runtime_env_name("abc123", 1),
            "testing-pool-abc123-1"
        );
        assert_eq!(
            resolve_pooled_runtime_env_name("abc123", 3),
            "testing-pool-abc123-3"
        );
    }

    #[test]
    fn pool_size_parser_returns_none_for_missing_or_invalid() {
        let empty = HashMap::<String, String>::new();
        assert_eq!(
            testing_runtime_pool_size_from_env(|name| empty.get(name).cloned()),
            None
        );

        let invalid =
            HashMap::from([("BERTH_TEST_RUNTIME_POOL_SIZE".to_owned(), "abc".to_owned())]);
        assert_eq!(
            testing_runtime_pool_size_from_env(|name| invalid.get(name).cloned()),
            None
        );
    }

    #[test]
    fn adaptive_pool_size_prefers_docker_limits_when_present() {
        let size = adaptive_pool_size_from_signals(
            Some(16),
            Some(32_u64 * 1024 * 1024 * 1024),
            Some(ResourceHint {
                cpus: 4,
                memory_bytes: 4_u64 * 1024 * 1024 * 1024,
            }),
        );
        assert_eq!(size, 8);
    }

    #[test]
    fn adaptive_pool_size_uses_host_signals_when_docker_absent() {
        let size =
            adaptive_pool_size_from_signals(Some(8), Some(16_u64 * 1024 * 1024 * 1024), None);
        assert_eq!(size, 8);
    }

    #[test]
    fn adaptive_pool_size_returns_sixteen_for_mid_tier_resources() {
        let size = adaptive_pool_size_from_signals(
            Some(16),
            Some(32_u64 * 1024 * 1024 * 1024),
            Some(ResourceHint {
                cpus: 20,
                memory_bytes: 48_u64 * 1024 * 1024 * 1024,
            }),
        );
        assert_eq!(size, 16);
    }

    #[test]
    fn adaptive_pool_size_returns_thirty_two_for_high_tier_resources() {
        let size = adaptive_pool_size_from_signals(
            Some(32),
            Some(96_u64 * 1024 * 1024 * 1024),
            Some(ResourceHint {
                cpus: 48,
                memory_bytes: 128_u64 * 1024 * 1024 * 1024,
            }),
        );
        assert_eq!(size, 32);
    }

    #[test]
    fn runtime_pool_names_are_unique_across_workspaces() {
        let root_a = temp_root("a");
        let root_b = temp_root("b");

        let lease_a = acquire_testing_runtime_lease(&root_a).expect("acquire lease a");
        let lease_b = acquire_testing_runtime_lease(&root_b).expect("acquire lease b");

        assert_ne!(lease_a.runtime_env_name(), lease_b.runtime_env_name());
    }

    #[test]
    fn workspace_pool_key_is_stable_for_same_path() {
        let root = temp_root("stable");
        assert_eq!(workspace_pool_key(&root), workspace_pool_key(&root));
    }

    #[test]
    fn stale_slot_with_dead_pid_is_reclaimed() {
        let root = temp_root("stale-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "424242\n").expect("write stale slot");

        assert!(should_reclaim_existing_slot_with(&slot_path, |_| false));
    }

    #[test]
    fn slot_with_live_pid_is_not_reclaimed() {
        let root = temp_root("live-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "12345\n").expect("write live slot");

        assert!(!should_reclaim_existing_slot_with(&slot_path, |_| true));
    }

    #[test]
    fn slot_without_pid_is_not_reclaimed() {
        let root = temp_root("invalid-slot");
        let slot_path = root.join("slot-1.lock");
        fs::write(&slot_path, "not-a-pid\n").expect("write invalid slot");

        assert!(!should_reclaim_existing_slot_with(&slot_path, |_| false));
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
