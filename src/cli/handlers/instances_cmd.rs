//! `berth ls`: every instance this installation owns, across projects.

use anyhow::Result;

use crate::instance::{InstanceRecord, list_instances, now_secs};

/// Lists instances as a table or one JSON document.
///
/// # Errors
///
/// Returns an error when the format is unknown or instances cannot be
/// listed.
pub(crate) fn handle_list_instances(format: &str, json: bool) -> Result<()> {
    // clap accepts `--format` in any case.
    let json = match (json, format.to_ascii_lowercase().as_str()) {
        (true, _) | (false, "json") => true,
        (false, "table") => false,
        (false, _) => anyhow::bail!("unknown --format '{format}'; use table or json"),
    };
    let instances = list_instances()?;
    let now = now_secs();
    if json {
        let rows: Vec<serde_json::Value> =
            instances.iter().map(|record| row(record, now)).collect();
        return super::serialize::print_json_pretty(&serde_json::json!({ "instances": rows }));
    }
    println!(
        "{:<20} {:<28} {:<8} {:<10} ROOT",
        "PROJECT", "INSTANCE", "STATUS", "EXPIRES"
    );
    for record in &instances {
        let expires = match record.lease.as_ref().and_then(|lease| lease.expires_at) {
            Some(at) if at <= now => "expired".to_owned(),
            Some(at) => format!("in {}m", (at - now).div_ceil(60)),
            None => "never".to_owned(),
        };
        let root = record
            .root
            .as_ref()
            .map_or_else(|| "-".to_owned(), |root| root.display().to_string());
        let gone = if record.owner_gone() { " (gone)" } else { "" };
        println!(
            "{:<20} {:<28} {:<8} {:<10} {root}{gone}",
            record.project,
            record.instance,
            record.status(),
            expires
        );
    }
    Ok(())
}

fn row(record: &InstanceRecord, now: u64) -> serde_json::Value {
    let lease = record.lease.as_ref();
    let expires_at = lease.and_then(|lease| lease.expires_at);
    serde_json::json!({
        "project": record.project,
        "instance": record.instance,
        "root": record.root.as_ref().map(|root| root.display().to_string()),
        "root_exists": record.root.as_ref().map(|root| root.exists()),
        "status": record.status(),
        "containers": record.containers.len(),
        "networks": record.networks,
        "running": record.containers.iter().filter(|c| c.state == "running").count(),
        "ttl_seconds": lease.and_then(|lease| lease.ttl_secs),
        "expires_at": expires_at,
        "expires_in_seconds": expires_at.map(|at| at.saturating_sub(now)),
        "expired": lease.is_some_and(|lease| lease.expired(now)),
        "lease_error": record.lease_error,
        "owner_pid": lease.and_then(|lease| lease.owner.as_ref()).map(|owner| owner.pid),
        "owner_alive": lease.and_then(|lease| lease.owner.as_ref()).map(|owner| owner.alive()),
    })
}
