//! docker ops prune module.
//!
//! Contains docker container prune operation used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::docker::{
    LABEL_MANAGED, VALUE_MANAGED_TRUE, inspect_label, inspect_status, is_dry_run,
    print_docker_command,
};
use crate::output::{self, LogLevel, Persistence};

use super::PruneOptions;
use super::common::run_docker_status;
use docker_cmd::{docker_output, ensure_success};
use preview::preview_global_prune_candidates;

mod docker_cmd;
mod preview;

pub(super) fn prune(options: PruneOptions<'_>) -> Result<()> {
    let mut args = vec!["container".to_owned(), "prune".to_owned()];
    if options.force {
        args.push("--force".to_owned());
    }
    for filter in options.filters {
        args.push("--filter".to_owned());
        args.push(filter.clone());
    }

    if is_dry_run() {
        preview_global_prune_candidates(options.filters)?;
    }
    run_docker_status(
        &args,
        &crate::docker::runtime_command_error_context("container prune"),
    )
}

pub(super) fn prune_stopped_container(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;
    if is_dry_run() {
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    let Some(status) = inspect_status(&container_name) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it was not found"),
            Persistence::Persistent,
        );
        return Ok(());
    };

    let Some(managed_label) = inspect_label(&container_name, LABEL_MANAGED) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because labels are unavailable"),
            Persistence::Persistent,
        );
        return Ok(());
    };
    if !is_berth_managed_label(&managed_label) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is not Berth-managed"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    if is_active_status(&status) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is {status}"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let rm_output = docker_output(
        &["rm".to_owned(), container_name.clone()],
        &crate::docker::runtime_command_error_context("rm"),
    )?;
    ensure_success(
        rm_output,
        &format!("Failed to prune container {container_name}"),
    )?;
    output::event(
        &service.name,
        LogLevel::Success,
        &format!("Pruned stopped container {container_name}"),
        Persistence::Persistent,
    );

    Ok(())
}

fn is_active_status(status: &str) -> bool {
    matches!(status, "running" | "paused" | "restarting")
}

fn is_berth_managed_label(value: &str) -> bool {
    value == VALUE_MANAGED_TRUE
}

#[cfg(test)]
mod tests {
    use super::{is_active_status, is_berth_managed_label};

    #[test]
    fn active_status_detection_matches_runtime_states() {
        assert!(is_active_status("running"));
        assert!(is_active_status("paused"));
        assert!(is_active_status("restarting"));
        assert!(!is_active_status("exited"));
    }

    #[test]
    fn managed_label_detection_requires_true_value() {
        assert!(is_berth_managed_label("true"));
        assert!(!is_berth_managed_label(""));
        assert!(!is_berth_managed_label("false"));
    }
}
