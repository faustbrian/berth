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
    let reaped = crate::instance::reap(true, None)?;
    let servers = crate::shared::collect_servers()?;
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
        return super::serialize::print_json_pretty(&serde_json::json!({
            "reaped_instances": reaped.instances,
            "removed_test_containers": reaped.slot_containers,
            "servers": servers,
        }));
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
    if servers.is_empty() && reaped.instances.is_empty() {
        output::event(
            "gc",
            LogLevel::Info,
            "Nothing to collect",
            Persistence::Persistent,
        );
    }
    Ok(())
}
