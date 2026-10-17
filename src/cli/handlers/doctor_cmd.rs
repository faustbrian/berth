//! cli handlers doctor cmd module.
//!
//! Contains cli handlers doctor cmd logic used by Berth command workflows.

use anyhow::Result;
use serde::Serialize;
use std::path::Path;

use super::serialize;
use crate::{cli, config};

pub(crate) struct HandleDoctorOptions<'a> {
    pub(crate) format: &'a str,
    pub(crate) fix: bool,
    pub(crate) repro: bool,
    pub(crate) reachability: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

#[derive(Serialize)]
struct DoctorResult {
    ok: bool,
    error: Option<String>,
    /// Every check, in the order it ran.
    checks: Vec<cli::support::DoctorCheck>,
}

pub(crate) fn handle_doctor(
    config: &config::Config,
    options: HandleDoctorOptions<'_>,
) -> Result<()> {
    let json = options.format.eq_ignore_ascii_case("json");
    if json {
        cli::support::start_doctor_recording();
    }
    let result = cli::support::run_doctor(
        config,
        cli::support::RunDoctorOptions {
            fix: options.fix,
            repro: options.repro,
            reachability: options.reachability,
            allow_stopped_app_runtime_checks: false,
            config_path: options.config_path,
            project_root: options.project_root,
        },
    );

    if json {
        let checks = cli::support::take_doctor_recording();
        match &result {
            Ok(()) => serialize::print_json_pretty(&DoctorResult {
                ok: true,
                error: None,
                checks,
            })?,
            Err(err) => serialize::print_json_pretty(&DoctorResult {
                ok: false,
                error: Some(err.to_string()),
                checks,
            })?,
        }
    }

    result
}
