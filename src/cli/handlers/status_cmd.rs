//! cli handlers status cmd module.
//!
//! Contains cli handlers status cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::{cli, config, display, docker};

pub(crate) fn handle_status(
    config: &config::Config,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let filtered = cli::support::filter_services(&config.service, kind, driver);
    match format {
        "json" => render_status_json(&filtered)?,
        _ => display::print_status(&filtered),
    }
    Ok(())
}

fn render_status_json(services: &[&config::ServiceConfig]) -> Result<()> {
    let statuses: Vec<serde_json::Value> =
        services.iter().map(|svc| build_status_json(svc)).collect();
    serialize::print_json_pretty(&statuses)
}

fn build_status_json(service: &config::ServiceConfig) -> serde_json::Value {
    let status = docker::inspect_service_status(service);
    serde_json::json!({
        "name": service.name,
        "kind": cli::support::kind_name(service.kind),
        "driver": cli::support::driver_name(service.driver),
        "image": service.image,
        "port": service.port,
        "status": status.unwrap_or_else(|| "not created".to_owned()),
    })
}
