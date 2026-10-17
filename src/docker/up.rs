//! docker up module.
//!
//! Contains docker up logic used by Berth command workflows.

use anyhow::Result;

use crate::config::{Kind, ServiceConfig};
use crate::docker::docker_image_exists;
use crate::output::{self, LogLevel, Persistence};

use super::{PullPolicy, inspect_status, is_dry_run};
use docker_cmd::{docker_output, docker_output_owned, ensure_success};

mod args_builder;
mod docker_cmd;
mod dry_run;
mod object_store_bucket;
mod state;

/// Ensures the service container is running.
pub fn up(service: &ServiceConfig, pull: PullPolicy, recreate: bool) -> Result<()> {
    let container_name = service.container_name()?;

    if is_dry_run() {
        return dry_run::describe(service, pull, recreate, &container_name);
    }

    if state::ensure_or_start_existing(&container_name, recreate)? {
        ensure_object_store_bucket_ready(service)?;
        return Ok(());
    }

    state::ensure_image_available(service, pull)?;

    let run_args = args_builder::build_run_args(service, &container_name);
    let output = docker_output_owned(&run_args, &super::runtime_command_error_context("run"))?;
    ensure_success(output, "Failed to start container")?;
    ensure_object_store_bucket_ready(service)?;

    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Started container {container_name}"),
        Persistence::Persistent,
    );
    Ok(())
}

fn ensure_object_store_bucket_ready(service: &ServiceConfig) -> Result<()> {
    if service.kind == Kind::ObjectStore {
        super::wait_until_healthy(service, 30, 2, None)?;
    }
    object_store_bucket::ensure_bucket_exists(service)
}

pub(super) fn inspect_image_exists(image: &str) -> Result<bool> {
    docker_image_exists(
        image,
        &super::runtime_command_error_context("image inspect"),
    )
}

/// Removes container as part of the docker up workflow.
pub(super) fn remove_container(container_name: &str) {
    drop(docker_output(
        &["rm", "-f", container_name],
        "Failed to remove container",
    ));
}

pub(super) fn start_container(container_name: &str) -> Result<bool> {
    if let Some(status) = inspect_status(container_name) {
        if status == "running" {
            output::event(
                container_name,
                LogLevel::Info,
                "Container already running",
                Persistence::Persistent,
            );
            return Ok(true);
        }

        if status == "exited" || status == "created" {
            let start_output = docker_output(
                &["start", container_name],
                &super::runtime_command_error_context("start"),
            )?;
            ensure_success(start_output, "Failed to start container")?;

            output::event(
                container_name,
                LogLevel::Success,
                "Started existing container",
                Persistence::Persistent,
            );
            return Ok(true);
        }

        remove_container(container_name);
    }

    Ok(false)
}
