//! The process that owns an instance, for leases that name one.
//!
//! An agent passes its own PID (`--owner-pid`, `BERTH_OWNER_PID`); once that
//! process is gone the instance is reaped. The process start time is kept
//! with the PID, so a reused PID never counts as the owner; it is read in a
//! form no time zone, locale or clock step changes.

use anyhow::Result;
use std::process::{Command, Stdio};

/// A process recorded as an instance's owner.
#[derive(Clone, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub(crate) struct OwnerProcess {
    pub(crate) pid: u32,
    /// `ps -o lstart=` of the process when it was recorded.
    pub(crate) started: String,
}

/// The owner named by `pid`.
///
/// # Errors
///
/// Returns an error when no such process runs or its start time cannot be
/// read.
pub(crate) fn owner_process(pid: u32) -> Result<OwnerProcess> {
    match probe(pid) {
        Probe::Running(started) => Ok(OwnerProcess { pid, started }),
        Probe::Gone => anyhow::bail!(
            "--owner-pid / BERTH_OWNER_PID {pid} is not a running process; pass the PID of a \
             process that lives as long as the instance should (your shell's `$$`, say)"
        ),
        Probe::Unknown(why) => anyhow::bail!("cannot read the start time of process {pid}: {why}"),
    }
}

impl OwnerProcess {
    /// Whether the recorded process may still run. Only a definite answer
    /// that no such process exists (or that the PID now belongs to a process
    /// started at another time) counts as dead: reaping deletes data, so an
    /// unreadable probe never does.
    #[must_use]
    pub(crate) fn alive(&self) -> bool {
        match probe(self.pid) {
            Probe::Running(started) => started == self.started,
            Probe::Gone => false,
            Probe::Unknown(_) => true,
        }
    }
}

enum Probe {
    /// The process runs; its start time, independent of time zone and
    /// locale.
    Running(String),
    /// No process has the PID.
    Gone,
    /// The probe itself failed.
    Unknown(String),
}

/// Reads a process's start time. Linux: start ticks since boot from
/// `/proc/{pid}/stat` (field 22), which no clock step or time zone changes.
/// Elsewhere (macOS): `ps -o lstart=` rendered in UTC with the C locale.
fn probe(pid: u32) -> Probe {
    let proc_root = std::path::Path::new("/proc/self/stat");
    if proc_root.exists() {
        return match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => stat
                // The command name may hold spaces and parentheses; fields
                // after the last `)` are positional (field 3 onwards).
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().nth(19))
                .map_or_else(
                    || Probe::Unknown("unexpected /proc stat".to_owned()),
                    |ticks| Probe::Running(format!("ticks:{ticks}")),
                ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Probe::Gone,
            Err(error) => Probe::Unknown(error.to_string()),
        };
    }
    let output = match Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .env("TZ", "UTC")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output,
        Err(error) => return Probe::Unknown(format!("ps: {error}")),
    };
    let started = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    match (output.status.code(), started.is_empty()) {
        (Some(0), false) => Probe::Running(format!("utc:{started}")),
        // `ps -p` exits 1 with no rows when the PID does not exist.
        (Some(1), true) => Probe::Gone,
        _ => Probe::Unknown(format!("ps exited {:?}", output.status.code())),
    }
}

#[cfg(test)]
mod tests {
    use super::{OwnerProcess, owner_process};

    #[test]
    fn a_running_process_owns_and_a_reused_pid_does_not() {
        let me = owner_process(std::process::id()).expect("this process runs");
        assert!(me.alive());
        let reused = OwnerProcess {
            pid: me.pid,
            started: "Thu Jan  1 00:00:00 1970".to_owned(),
        };
        assert!(!reused.alive(), "same PID, different process");
        assert!(owner_process(u32::MAX).is_err());
    }
}
