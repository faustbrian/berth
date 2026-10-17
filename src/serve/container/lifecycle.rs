//! Concrete `docker run`/reuse/remove lifecycle for serve containers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

use super::super::{mailhog_smtp_port, should_inject_frankenphp_server_name};
use args::build_run_args;
use docker_cmd::force_remove_container;
use dry_run::print_dry_run_container_start;
use existing::handle_existing_container;
use start::start_new_container;

mod args;
pub(crate) use args::container_env_pairs;
mod docker_cmd;
mod dry_run;
mod existing;
mod remove;
mod start;

/// Ensures a serve container is running, creating or reusing as needed.
///
/// Behavior order:
/// - In dry-run mode, only logs planned actions.
/// - With `recreate`, forcibly removes existing container before run.
/// - Otherwise attempts reuse/start of existing container before creating a new one.
pub(super) fn ensure_container_running(
    target: &ServiceConfig,
    recreate: bool,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let target = &*crate::network::with_existing_publication(target);
    let container_name = target.container_name()?;
    crate::network::ensure_service_network(target)?;

    if crate::docker::is_dry_run() {
        print_dry_run_container_start(
            target,
            &container_name,
            recreate,
            runtime_image,
            mailhog_smtp_port(target),
        )?;
        return Ok(());
    }

    if recreate || crate::network::publication_drifted(target) {
        force_remove_container(&container_name);
    } else if handle_existing_container(&container_name)? {
        return Ok(());
    }

    let run_args = build_run_args(
        target,
        runtime_image,
        project_root,
        injected_env,
        should_inject_frankenphp_server_name(target, injected_env),
    )?;

    start_new_container(target, &run_args)
}

/// Stops and removes the serve container for a target.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    remove::remove_container(target)
}
