//! cli handlers list cmd module.
//!
//! Contains cli handlers list cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::{cli, config};

pub(crate) fn handle_list(
    config: &config::Config,
    format: &str,
    kind: Option<config::Kind>,
    driver: Option<config::Driver>,
) -> Result<()> {
    let filtered = cli::support::filter_services(&config.service, kind, driver);
    match format {
        "json" => render_list_json(&filtered)?,
        _ => render_list_text(&filtered),
    }
    Ok(())
}

fn render_list_json(services: &[&config::ServiceConfig]) -> Result<()> {
    let names: Vec<&str> = services.iter().map(|svc| svc.name.as_str()).collect();
    serialize::print_json_pretty(&names)
}

fn render_list_text(services: &[&config::ServiceConfig]) {
    for svc in services {
        println!("{}", svc.name);
    }
}
