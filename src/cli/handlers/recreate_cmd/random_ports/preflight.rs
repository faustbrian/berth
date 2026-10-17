//! Preflight preparation for recreate random-port runtime flow.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

use super::planning::plan_randomized_runtimes;

pub(super) type PreparedRecreateRuntime = cli::support::PreparedRandomPorts<config::ServiceConfig>;

#[expect(
    clippy::too_many_arguments,
    reason = "7.0.0 signature kept as is; it predates the 9.x lint baseline (L154)"
)]
pub(super) fn prepare_recreate_runtime(
    config: &config::Config,
    service: Option<&str>,
    services: &[String],
    kind: Option<config::Kind>,
    profile: Option<&str>,
    env_output: bool,
    runtime_env: Option<&str>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<PreparedRecreateRuntime> {
    cli::support::prepare_random_ports(
        config,
        env_output,
        runtime_env,
        config_path,
        project_root,
        |config| {
            let (planned, runtime_config) =
                plan_randomized_runtimes(config, service, services, kind, profile)?;
            let app_env =
                cli::support::runtime_app_env(&runtime_config, &std::collections::HashMap::new())?;
            Ok((planned, app_env))
        },
    )
}
