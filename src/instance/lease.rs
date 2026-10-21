//! Instance leases: who owns an instance and until when.
//!
//! A lease lives at `~/.berth/instances/{project}/{instance}/lease.json`
//! with the checkout that owns the instance and, when `up --ttl` set one, a
//! time-to-live renewed by every berth command in that checkout. Without a
//! TTL an instance is reaped only when its checkout is gone or its named
//! owner process exits, so an idle human environment is never deleted.

use anyhow::{Context, Result};

use super::TtlChange;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const FILE_NAME: &str = "lease.json";

/// An instance's lease.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Lease {
    /// The checkout that owns the instance.
    pub(crate) root: PathBuf,
    /// The TTL in seconds, renewed by each berth command in the checkout.
    pub(crate) ttl_secs: Option<u64>,
    /// When the instance expires (seconds since the epoch), if it has a TTL.
    pub(crate) expires_at: Option<u64>,
    /// The process that owns the instance, if one was named.
    #[serde(default)]
    pub(crate) owner: Option<super::owner::OwnerProcess>,
}

impl Lease {
    /// Whether the process named as the instance's owner has exited.
    #[must_use]
    pub(crate) fn owner_dead(&self) -> bool {
        self.owner.as_ref().is_some_and(|owner| !owner.alive())
    }

    /// Whether the lease has expired at `now` (seconds since the epoch).
    #[must_use]
    pub(crate) fn expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|expires| expires <= now)
    }
}

/// Seconds since the epoch.
#[must_use]
pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn lease_dir(project: &str, instance: &str) -> PathBuf {
    Path::new("instances").join(project).join(instance)
}

/// Reads an instance's lease, or `None` when it has none.
///
/// # Errors
///
/// Returns an error when the lease exists but cannot be read or parsed.
pub(crate) fn read_lease(project: &str, instance: &str) -> Result<Option<Lease>> {
    let path = crate::home::home_path(&lease_dir(project, instance).join(FILE_NAME))?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    serde_json::from_str(&raw)
        .map(Some)
        .with_context(|| format!("{} is not a valid lease; delete it", path.display()))
}

/// Writes an instance's lease for `root`: `ttl` and `owner` replace the TTL
/// and owner process when given, otherwise the existing ones are kept;
/// either way the expiry is renewed. With `take_over` (`up`), an existing
/// owner that has exited is dropped rather than carried forward: whoever
/// brings the instance up now uses it, and a dead owner would get it reaped
/// under them. Other commands keep the owner, alive or not, so the instance
/// is still reaped once a command that outlived its killed agent ends.
/// The read-modify-write runs under a per-instance host lock, so concurrent
/// commands never drop each other's TTL or owner.
///
/// # Errors
///
/// Returns an error when the lease cannot be read or written.
pub(crate) fn renew_lease(
    project: &str,
    instance: &str,
    root: &Path,
    ttl: TtlChange,
    owner: Option<super::owner::OwnerProcess>,
    take_over: bool,
) -> Result<Lease> {
    let _lock = crate::home::HostLock::acquire(&format!("lease_{project}_{instance}"))?;
    let existing = read_lease(project, instance)?;
    let ttl_secs = match ttl {
        TtlChange::Set(ttl) => Some(ttl.as_secs()),
        TtlChange::Clear => None,
        TtlChange::Keep => existing.as_ref().and_then(|lease| lease.ttl_secs),
    };
    let lease = Lease {
        root: root.to_path_buf(),
        ttl_secs,
        expires_at: ttl_secs.map(|ttl| now_secs().saturating_add(ttl)),
        owner: owner.or_else(|| {
            existing
                .as_ref()
                .and_then(|lease| lease.owner.clone())
                .filter(|owner| !take_over || owner.alive())
        }),
    };
    if existing.as_ref() == Some(&lease) {
        return Ok(lease);
    }
    let dir = crate::home::instance_dir(project, instance)?;
    let staged = dir.join(format!(".{FILE_NAME}.{}", std::process::id()));
    let body = serde_json::to_string_pretty(&lease).context("failed to render the lease")?;
    std::fs::write(&staged, body)
        .with_context(|| format!("failed to write {}", staged.display()))?;
    std::fs::rename(&staged, dir.join(FILE_NAME))
        .with_context(|| format!("failed to write the lease in {}", dir.display()))?;
    Ok(lease)
}

/// Parses `--ttl`: a whole number with `s`, `m`, `h` or `d` (`90m`, `2h`),
/// or `none`, which clears a TTL set earlier.
///
/// # Errors
///
/// Returns an error for anything else, or a zero TTL.
pub(crate) fn parse_ttl(raw: &str) -> Result<TtlChange> {
    let raw = raw.trim();
    if raw == "none" {
        return Ok(TtlChange::Clear);
    }
    let (digits, unit) = raw.split_at(raw.len().saturating_sub(1));
    let scale = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => anyhow::bail!("TTL '{raw}' needs a unit: s, m, h or d (for example 2h)"),
    };
    let count: u64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("TTL '{raw}' must be a whole number and a unit, like 90m"))?;
    if count == 0 {
        anyhow::bail!("TTL '{raw}' must be longer than zero");
    }
    Ok(TtlChange::Set(Duration::from_secs(
        count.saturating_mul(scale),
    )))
}

#[cfg(test)]
mod tests {
    use super::{Lease, TtlChange, parse_ttl};
    use crate::home::same_file;

    #[test]
    fn a_lock_on_a_deleted_file_is_not_the_lock_at_the_path() {
        let dir = crate::test_support::temp_root()
            .join(format!("berth-lease-inode-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("instance_shop_a.lock");
        let first = std::fs::File::create(&path).expect("lock file");
        assert!(same_file(&first, &path).expect("compare"));
        std::fs::remove_file(&path).expect("purge deletes it");
        assert!(
            !same_file(&first, &path).expect("compare"),
            "the path is gone"
        );
        let second = std::fs::File::create(&path).expect("a later command's lock file");
        assert!(
            !same_file(&first, &path).expect("compare"),
            "another file now"
        );
        assert!(same_file(&second, &path).expect("compare"));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn ttls_need_a_unit_and_a_positive_count_or_are_none() {
        let set = |seconds| TtlChange::Set(Duration::from_secs(seconds));
        assert_eq!(parse_ttl("90m").expect("ttl"), set(5400));
        assert_eq!(parse_ttl("2h").expect("ttl"), set(7200));
        assert_eq!(parse_ttl("1d").expect("ttl"), set(86_400));
        assert_eq!(parse_ttl("none").expect("ttl"), TtlChange::Clear);
        for bad in [
            "", "2", "h", "0h", "-1h", "1.5h", "2w", "none1", "nonee", "None",
        ] {
            assert!(parse_ttl(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_a_lease_with_a_ttl_expires() {
        let lease = Lease {
            root: PathBuf::from("/w"),
            ttl_secs: Some(60),
            expires_at: Some(100),
            owner: None,
        };
        assert!(!lease.expired(99));
        assert!(lease.expired(100));
        let forever = Lease {
            root: PathBuf::from("/w"),
            ttl_secs: None,
            expires_at: None,
            owner: None,
        };
        assert!(!forever.expired(u64::MAX));
    }
}

/// A berth command running in an instance: a shared `flock` held for the
/// command's whole run, so the reaper never deletes an instance in use
/// however long the command takes (a test run past its TTL included).
#[derive(Debug)]
pub(crate) struct InUse {
    _file: std::fs::File,
}

/// `locks/instance_{project}_{instance}.lock`: `_` occurs in neither part,
/// so no two instances share a lock.
fn in_use_lock_path(project: &str, instance: &str) -> Result<PathBuf> {
    Ok(crate::home::private_dir(Path::new("locks"))?
        .join(format!("instance_{project}_{instance}.lock")))
}

/// Removes the instance's in-use lock file after a purge (the purging
/// process holds the lock, so no command can be using it).
///
/// # Errors
///
/// Returns an error when the file exists but cannot be removed.
pub(crate) fn remove_in_use_lock(project: &str, instance: &str) -> Result<()> {
    let path = in_use_lock_path(project, instance)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn in_use_lock(project: &str, instance: &str) -> Result<std::fs::File> {
    let path = in_use_lock_path(project, instance)?;
    std::os::unix::fs::OpenOptionsExt::mode(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false),
        0o600,
    )
    .open(&path)
    .with_context(|| format!("failed to open {}", path.display()))
}

/// Marks the instance in use until the guard drops, waiting while the
/// reaper deletes it.
///
/// # Errors
///
/// Returns an error when the lock file cannot be opened or locked.
pub(crate) fn hold_in_use(project: &str, instance: &str) -> Result<InUse> {
    let path = in_use_lock_path(project, instance)?;
    loop {
        let file = in_use_lock(project, instance)?;
        file.lock_shared()
            .with_context(|| format!("failed to mark {project}/{instance} in use"))?;
        // A purge deletes the lock file while holding it; a command that was
        // waiting would then hold a lock no later reaper can see. Keep the
        // lock only on the file that is at the path now.
        if crate::home::same_file(&file, &path)? {
            return Ok(InUse { _file: file });
        }
    }
}

/// Locks the instance for reaping: `None` while any command uses it.
///
/// # Errors
///
/// Returns an error when the lock file cannot be opened or probed.
pub(crate) fn try_lock_for_reaping(project: &str, instance: &str) -> Result<Option<std::fs::File>> {
    let file = in_use_lock(project, instance)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("failed to lock {project}/{instance} for reaping"))
        }
    }
}
