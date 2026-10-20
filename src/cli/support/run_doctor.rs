//! cli support run doctor module.
//!
//! Contains cli support run doctor logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config;

mod app;
mod docker;
mod ports;
mod repair;
mod report;
pub(crate) use report::{DoctorCheck, start_recording, take_recording};
mod repro;
mod secrets;

/// Runs the doctor support workflow.
pub(crate) struct RunDoctorOptions<'a> {
    pub(crate) fix: bool,
    pub(crate) repro: bool,
    pub(crate) reachability: bool,
    pub(crate) allow_stopped_app_runtime_checks: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn run_doctor(config: &config::Config, options: RunDoctorOptions<'_>) -> Result<()> {
    let mut has_error = false;

    has_error |= docker::check_runtime_availability();
    has_error |= ports::check_port_conflicts(config);
    has_error |= app::check_app_services(
        config,
        options.fix,
        options.allow_stopped_app_runtime_checks,
    );
    if options.reachability {
        has_error |= app::check_reachability(config);
    }

    if options.fix {
        has_error |= repair::repair_unhealthy_services(config);
    }

    has_error |= secrets::check_sensitive_env_values(options.config_path, options.project_root)?;
    if options.repro {
        has_error |=
            repro::check_reproducibility(config, options.config_path, options.project_root)?;
    }

    if has_error {
        anyhow::bail!(
            "doctor found the issues listed above; fix them and run the same command again ({} \
             repairs what it can)",
            crate::followup::command("doctor --fix")
        )
    }

    report::success("Doctor checks passed");
    Ok(())
}
