//! `berth gc`: reap now, then remove shared servers nothing uses.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};

/// Reaps expired and orphaned instances regardless of the reaping interval,
/// then removes unused shared servers.
///
/// # Errors
///
/// Returns an error when reaping or a removal fails.
pub(crate) fn handle_gc(json: bool) -> Result<()> {
    // Deciding what is unused needs real Docker reads, which `--dry-run`
    // stubs out: refuse instead of reporting guesses (L136).
    if crate::docker::is_dry_run() {
        anyhow::bail!("`berth gc` does not support --dry-run; `berth ls --json` shows what exists");
    }
    let reaped = crate::instance::reap(true, None)?;
    let slot_locks = reaped.slot_locks;
    let servers = crate::shared::collect_servers()?;
    // A gateway failure must not hide what was already reaped and removed:
    // it is reported with the rest, then fails the command.
    let (gateway, gateway_failure) = match crate::gateway::remove_gateway_if_unused() {
        Ok(outcome) => (outcome, None),
        Err(error) => (Err(format!("failed: {error:#}")), Some(error)),
    };
    if json {
        let servers: Vec<serde_json::Value> = servers
            .iter()
            .map(|verdict| {
                serde_json::json!({
                    "server": verdict.server,
                    "engine": verdict.engine,
                    "removed": verdict.removed,
                    "reason": verdict.reason,
                })
            })
            .collect();
        super::serialize::print_json_pretty(&serde_json::json!({
            "reaped_instances": reaped.instances,
            "removed_test_containers": reaped.slot_containers,
            "removed_test_slot_locks": slot_locks,
            "servers": servers,
            "gateway": match &gateway {
                Ok(name) => serde_json::json!({ "removed": true, "container": name }),
                Err(reason) => serde_json::json!({ "removed": false, "reason": reason }),
            },
        }))?;
        return gateway_failure.map_or(Ok(()), Err);
    }
    for verdict in &servers {
        let (level, action) = if verdict.removed {
            (LogLevel::Success, "Removed")
        } else {
            (LogLevel::Info, "Kept")
        };
        output::event(
            "gc",
            level,
            &format!("{action} {} ({})", verdict.server, verdict.reason),
            Persistence::Persistent,
        );
    }
    if slot_locks > 0 {
        output::event(
            "gc",
            LogLevel::Success,
            &format!("Removed {slot_locks} unused test-slot lock files"),
            Persistence::Persistent,
        );
    }
    if let Ok(name) = &gateway {
        output::event(
            "gc",
            LogLevel::Success,
            &format!("Removed the unused gateway {name}"),
            Persistence::Persistent,
        );
    }
    if servers.is_empty()
        && reaped.instances.is_empty()
        && slot_locks == 0
        && gateway.is_err()
        && gateway_failure.is_none()
    {
        output::event(
            "gc",
            LogLevel::Info,
            "Nothing to collect",
            Persistence::Persistent,
        );
    }
    gateway_failure.map_or(Ok(()), Err)
}
