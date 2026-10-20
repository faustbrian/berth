//! Crash recovery without a daemon: berth commands delete the instances
//! nobody can use any more.
//!
//! An instance this installation owns is reaped (purged like `down
//! --purge`) when its lease expired, its named owner process exited, or its
//! checkout no longer exists. Test
//! slots whose run died (their `flock` is free) lose their leftover
//! containers. Reaping runs at most every [`INTERVAL`] unless forced, and
//! only one process reaps at a time.

use anyhow::{Context, Result};
use std::time::Duration;

use super::catalog::{InstanceRecord, list_instances};
use crate::output::{self, LogLevel, Persistence};

/// How often ordinary commands reap. Listing instances costs one `docker
/// ps`; 30 seconds keeps 30 parallel agents from repeating it constantly.
const INTERVAL: Duration = Duration::from_secs(30);

/// What a reaping pass deleted.
#[derive(Debug, Default)]
pub(crate) struct ReapReport {
    /// `{project}/{instance}` of every reaped instance.
    pub(crate) instances: Vec<String>,
    /// Names of removed leftover test-slot containers.
    pub(crate) slot_containers: Vec<String>,
    /// How many unused test-slot lock files a forced pass deleted.
    pub(crate) slot_locks: usize,
}

/// Reaps unless another process is reaping or, without `force`, a pass ran
/// within [`INTERVAL`]. `keep` names an instance that must survive (the one
/// the command runs in).
///
/// # Errors
///
/// Returns an error when Docker or berth state cannot be read, or a reaped
/// instance cannot be deleted completely.
pub(crate) fn reap(force: bool, keep: Option<(&str, &str)>) -> Result<ReapReport> {
    if crate::docker::is_dry_run() {
        return Ok(ReapReport::default());
    }
    // A forced pass (`berth gc`) waits for a pass in progress instead of
    // reporting that nothing was reaped; ordinary commands never wait.
    let _lock = if force {
        crate::home::HostLock::acquire("reap")?
    } else {
        let Some(lock) = crate::home::HostLock::try_acquire("reap")? else {
            return Ok(ReapReport::default());
        };
        lock
    };
    let stamp = crate::home::private_dir(std::path::Path::new("locks"))?.join("reap.stamp");
    if !force && ran_recently(&stamp) {
        return Ok(ReapReport::default());
    }
    std::fs::write(&stamp, b"").with_context(|| format!("failed to write {}", stamp.display()))?;

    let now = super::lease::now_secs();
    let mut report = ReapReport::default();
    let mut failures = Vec::new();
    for record in list_instances()? {
        let kept = keep == Some((record.project.as_str(), record.instance.as_str()));
        if !kept && reap_reason(&record, now).is_some() {
            // A command running in the instance (a long test run) keeps it.
            let Some(_reaping) =
                super::lease::try_lock_for_reaping(&record.project, &record.instance)?
            else {
                continue;
            };
            // The listing may be stale: a command in the instance can have
            // renewed it since. Decide again from the lease as it is now,
            // while holding the lock that keeps commands out.
            let mut current = record.clone();
            current.lease = match super::lease::read_lease(&record.project, &record.instance) {
                Ok(lease) => lease,
                Err(error) => {
                    failures.push(format!("{error:#}"));
                    continue;
                }
            };
            if let Some(lease) = &current.lease {
                current.root = Some(lease.root.clone());
            }
            let Some(why) = reap_reason(&current, super::lease::now_secs()) else {
                continue;
            };
            match purge(&current) {
                Ok(()) => {
                    output::event(
                        "reap",
                        LogLevel::Info,
                        &format!(
                            "Reaped {}/{} because {why}",
                            current.project, current.instance
                        ),
                        Persistence::Persistent,
                    );
                    report
                        .instances
                        .push(format!("{}/{}", current.project, current.instance));
                }
                Err(error) => failures.push(format!("{error:#}")),
            }
            continue;
        }
        match reap_stale_slots(&record) {
            Ok(removed) => report.slot_containers.extend(removed),
            Err(error) => failures.push(format!("{error:#}")),
        }
    }
    if let Some(first) = failures.into_iter().next() {
        anyhow::bail!("failed to reap: {first}");
    }
    // `berth gc` also deletes slot locks no run holds: under this pass's
    // lock, so no other reaper uses them meanwhile, and never for a slot
    // that still has containers (a dead run's that this pass skipped, say),
    // which a later pass finds only through its lock file.
    if force {
        let mut occupied = std::collections::BTreeSet::new();
        for record in list_instances()? {
            for container in &record.containers {
                if let Some((workspace, slot)) = slot_of(&container.name) {
                    occupied.insert(format!("{workspace}/slot-{slot}.lock"));
                }
            }
        }
        report.slot_locks = super::free_test_slots::remove_free_test_slot_locks(&occupied)?;
        // Lock files of instances that have neither state nor containers.
        let known = list_instances()?
            .into_iter()
            .map(|record| (record.project, record.instance))
            .collect();
        super::free_instance_locks::remove_orphan_instance_locks(&known)?;
    }
    Ok(report)
}

/// Why the instance should be reaped at `now`, if it should.
fn reap_reason(record: &InstanceRecord, now: u64) -> Option<&'static str> {
    if record.lease_error.is_some() {
        return None;
    }
    let lease = record.lease.as_ref();
    if lease.is_some_and(|lease| lease.expired(now)) {
        Some("its lease expired")
    } else if lease.is_some_and(super::lease::Lease::owner_dead) {
        Some("its owner process exited")
    } else if record.owner_gone() {
        Some("its checkout is gone")
    } else {
        None
    }
}

fn ran_recently(stamp: &std::path::Path) -> bool {
    std::fs::metadata(stamp)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed < INTERVAL)
}

/// Deletes everything of the instance, as `down --purge` does. Its berth
/// state (the lease) goes last and only once the tenants and Docker objects
/// are gone, so an instance that could not be deleted completely stays
/// reapable and the next pass (or `berth gc`) retries it.
fn purge(record: &InstanceRecord) -> Result<()> {
    let tenants = crate::shared::drop_instance_tenants(&record.project, &record.instance);
    let objects = crate::docker::purge_instance_objects(&record.project, &record.instance);
    tenants.and(objects).map_err(|error| {
        anyhow::anyhow!(
            "{error:#}; the instance is kept, so fix the cause and the next berth command (or \
             {}) retries",
            crate::followup::global("gc")
        )
    })?;
    crate::home::remove_instance_state(&record.project, &record.instance)?;
    super::lease::remove_in_use_lock(&record.project, &record.instance)
}

/// Removes containers of test slots (`…-testing-pool-{workspace}-{slot}`)
/// whose lock nobody holds: the run that made them died. The slot's
/// tenants are dropped by its next run's reset or the instance's purge.
fn reap_stale_slots(record: &InstanceRecord) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for container in &record.containers {
        let Some((workspace, slot)) = slot_of(&container.name) else {
            continue;
        };
        // `private_dir` creates the workspace's directory 0700, refusing
        // symlinks, should `berth gc` have emptied it.
        let lock = crate::home::private_dir(
            &std::path::Path::new("locks")
                .join("test-slots")
                .join(workspace),
        )?
        .join(format!("slot-{slot}.lock"));
        // Hold the slot while removing its containers, so a run that takes
        // the slot meanwhile never loses the containers it just started. A
        // missing lock file (deleted by `berth gc` after the run started) is
        // created: its containers are still the dead run's.
        let Some(_slot) = hold_slot(&lock)? else {
            continue;
        };
        let output = crate::docker::run_docker_output(
            &["rm", "-f", "-v", &container.name],
            "failed to remove a stale test container",
        )?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
        if !output.status.success() && !stderr.contains("no such container") {
            anyhow::bail!(
                "failed to remove stale test container {}: {}",
                container.name,
                stderr.trim()
            );
        }
        removed.push(container.name.clone());
    }
    Ok(removed)
}

/// Takes a slot's lock without waiting, creating its file if needed, and
/// only on the file that is at the path afterwards: `None` while a run
/// holds it.
fn hold_slot(path: &std::path::Path) -> Result<Option<std::fs::File>> {
    use std::os::unix::fs::OpenOptionsExt as _;
    loop {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(error).with_context(|| format!("failed to lock {}", path.display()));
            }
        }
        if crate::home::same_file(&file, path)? {
            return Ok(Some(file));
        }
    }
}

/// The workspace key and slot of a test-slot container name.
fn slot_of(name: &str) -> Option<(&str, &str)> {
    let (_, rest) = name.split_once("-testing-pool-")?;
    let (workspace, slot) = rest.rsplit_once('-')?;
    let valid = workspace.len() == 16
        && workspace.bytes().all(|byte| byte.is_ascii_hexdigit())
        && !slot.is_empty()
        && slot.bytes().all(|byte| byte.is_ascii_digit());
    valid.then_some((workspace, slot))
}

#[cfg(test)]
mod tests {
    use super::slot_of;

    #[test]
    fn slot_containers_are_recognized_by_name() {
        assert_eq!(
            slot_of("berth_shop_a-1_app-testing-pool-72d34fb23814d2bd-3"),
            Some(("72d34fb23814d2bd", "3"))
        );
        assert_eq!(slot_of("berth_shop_a-1_app"), None);
        assert_eq!(slot_of("berth_shop_a-1_app-testing-pool-xyz-3"), None);
    }
}
