//! cli handlers profile cmd module.
//!
//! Contains cli handlers profile cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::cli::handlers::serialize;
use crate::{cli, config};

/// Handles the `profile list` CLI command.
pub(crate) fn handle_profile_list() {
    for name in cli::support::profile_names() {
        println!("{name}");
    }
}

/// Handles the `profile show` CLI command.
pub(crate) fn handle_profile_show(config: &config::Config, name: &str, format: &str) -> Result<()> {
    let targets = cli::support::resolve_profile_targets(config, name)?;
    match format {
        "json" => {
            let values: Vec<serde_json::Value> = targets
                .iter()
                .map(|svc| {
                    serde_json::json!({
                        "name": svc.name,
                        "kind": cli::support::kind_name(svc.kind),
                        "driver": cli::support::driver_name(svc.driver),
                    })
                })
                .collect();
            serialize::print_json_pretty(&values)?;
        }
        "markdown" => {
            println!("| service | kind | driver |");
            println!("|---|---|---|");
            for svc in targets {
                println!(
                    "| {} | {} | {} |",
                    svc.name,
                    cli::support::kind_name(svc.kind),
                    cli::support::driver_name(svc.driver)
                );
            }
        }
        _ => {
            for svc in targets {
                println!(
                    "{}\t{}\t{}",
                    svc.name,
                    cli::support::kind_name(svc.kind),
                    cli::support::driver_name(svc.driver)
                );
            }
        }
    }
    Ok(())
}
