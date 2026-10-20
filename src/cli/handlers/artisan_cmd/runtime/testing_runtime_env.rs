//! Unique testing runtime environment naming helpers.

use anyhow::{Context, Result};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;
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

/// A held test slot: an exclusive `flock` on the slot's lock file, released
/// when the run ends or its process dies (a killed agent never leaves a
/// stale slot; ledger decision D6).
struct TestingRuntimeSlot {
    _file: std::fs::File,
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
    let lock_root = pool_lock_root(&workspace_key)?;
    // A run waits as long as every slot is busy: other test runs end, and a
    // killed one frees its slot, so the wait has no deadline.
    let mut notified = false;
    loop {
        for slot in 1..=pool_size {
            if let Some(file) = try_lock_slot(&lock_root.join(format!("slot-{slot}.lock")))? {
                return Ok(TestingRuntimeLease {
                    runtime_env: resolve_pooled_runtime_env_name(&workspace_key, slot),
                    _slot: Some(TestingRuntimeSlot { _file: file }),
                });
            }
        }
        if !notified {
            notified = true;
            crate::output::event(
                "test",
                crate::output::LogLevel::Info,
                &format!(
                    "all {pool_size} test slots of this workspace are busy; waiting for a \
                     running test command (BERTH_TEST_RUNTIME_POOL_SIZE raises the pool)"
                ),
                crate::output::Persistence::Persistent,
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Takes the slot's lock without waiting: `None` when another run holds it.
fn try_lock_slot(path: &Path) -> Result<Option<std::fs::File>> {
    loop {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        match lock_opened(file, path)? {
            SlotLock::Held(file) => return Ok(Some(file)),
            SlotLock::Busy => return Ok(None),
            // `berth gc` deleted it between the open and the lock.
            SlotLock::Stale => {}
        }
    }
}

enum SlotLock {
    Held(std::fs::File),
    Busy,
    Stale,
}

/// Locks an opened slot file, keeping the lock only while that file is the
/// one at the path.
fn lock_opened(file: std::fs::File, path: &Path) -> Result<SlotLock> {
    match file.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return Ok(SlotLock::Busy),
        Err(std::fs::TryLockError::Error(error)) => {
            return Err(error).with_context(|| format!("failed to lock {}", path.display()));
        }
    }
    if crate::home::same_file(&file, path)? {
        Ok(SlotLock::Held(file))
    } else {
        Ok(SlotLock::Stale)
    }
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
    let output = crate::docker::engine_command()
        .ok()?
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

/// The workspace's slot locks, under the berth home (unit tests use a
/// private temporary directory instead of the developer's home).
fn pool_lock_root(workspace_key: &str) -> Result<std::path::PathBuf> {
    #[cfg(test)]
    {
        let root = crate::test_support::temp_root()
            .join("test-slots")
            .join(workspace_key);
        std::fs::create_dir_all(&root)
            .with_context(|| format!("failed to create {}", root.display()))?;
        Ok(root)
    }
    #[cfg(not(test))]
    crate::home::private_dir(&Path::new("locks").join("test-slots").join(workspace_key))
}

fn resolve_pooled_runtime_env_name(workspace_key: &str, slot: usize) -> String {
    format!("testing-pool-{workspace_key}-{slot}")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        ResourceHint, SlotLock, acquire_testing_runtime_lease, adaptive_pool_size_from_signals,
        lock_opened, resolve_pooled_runtime_env_name, testing_runtime_env_name,
        testing_runtime_pool_size_from_env, try_lock_slot, workspace_pool_key,
    };

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
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
    fn a_slot_whose_lock_file_was_deleted_is_not_held() {
        let root = temp_root("deleted");
        fs::create_dir_all(&root).expect("root");
        let path = root.join("slot-1.lock");
        // A run opened the file; `berth gc` deleted it before the run locked.
        fs::write(&path, b"").expect("lock file");
        let opened = fs::File::open(&path).expect("open");
        fs::remove_file(&path).expect("gc removes it");
        assert!(matches!(
            lock_opened(opened, &path).expect("lock"),
            SlotLock::Stale
        ));
        let held = try_lock_slot(&path).expect("lock").expect("free slot");
        assert!(crate::home::same_file(&held, &path).expect("same"));
    }

    #[test]
    fn a_held_slot_is_skipped_and_freed_when_its_holder_ends() {
        let root = temp_root("held");
        let first = acquire_testing_runtime_lease(&root).expect("first lease");
        let second = acquire_testing_runtime_lease(&root).expect("second lease");
        assert_ne!(
            first.runtime_env_name(),
            second.runtime_env_name(),
            "a held slot is skipped"
        );
        let freed = first.runtime_env_name().to_owned();
        drop(first);
        let third = acquire_testing_runtime_lease(&root).expect("third lease");
        assert_eq!(
            third.runtime_env_name(),
            freed,
            "the slot frees as soon as its holder ends"
        );
    }
}
