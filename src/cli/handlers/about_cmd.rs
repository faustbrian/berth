//! cli handlers about cmd module.
//!
//! Contains cli handlers about cmd logic used by Berth command workflows.

use anyhow::Result;
use serde_json::json;
use std::path::Path;

use super::serialize;
use crate::{cli, config, display};

pub(crate) fn handle_about(
    config: &config::Config,
    format: &str,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    let project_root = cli::support::workspace_root(config_path, project_root)?;
    let config_path = config_path
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(|| {
            config::config_path_in_dir(&project_root)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "missing Berth config (.berth.toml or .berth.yaml) at {}",
                    project_root.display()
                )
            })
        })?;

    if format.eq_ignore_ascii_case("json") {
        let app_name = project_root
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("unknown");
        let runtime = runtime_env.unwrap_or("default");
        let dry_run = crate::docker::is_dry_run();
        let services: Vec<_> = config
            .service
            .iter()
            .map(|service| {
                let container_name = service
                    .container_name()
                    .unwrap_or_else(|_| "<unresolved>".to_owned());
                let status = crate::docker::inspect_status(&container_name)
                    .unwrap_or_else(|| "not created".to_owned());
                json!({
                    "name": service.name,
                    "kind": format!("{:?}", service.kind),
                    "driver": format!("{:?}", service.driver),
                    "container": container_name,
                    "status": status
                })
            })
            .collect();
        return serialize::print_json_pretty(&json!({
            "application": {
                "name": app_name,
                "version": env!("CARGO_PKG_VERSION"),
                "runtime_env": runtime,
                "project_root": project_root,
                "config_path": config_path,
                "dry_run": dry_run
            },
            "environment": {
                "container_prefix": config.container_prefix.as_deref(),
                "service_count": config.service.len(),
                "swarm_target_count": config.swarm.len()
            },
            "services": services
        }));
    }

    display::print_about(config, &project_root, &config_path, runtime_env);
    Ok(())
}
