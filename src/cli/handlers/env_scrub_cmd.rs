//! cli handlers env scrub cmd module.
//!
//! Contains cli handlers env scrub cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli;
use crate::output::{self, LogLevel, Persistence};

/// Handles the `env scrub` CLI command.
pub(crate) fn handle_env_scrub(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    env_file: Option<&Path>,
    runtime_env: Option<&str>,
) -> Result<()> {
    let env_path =
        cli::support::default_env_path(config_path, project_root, env_file, runtime_env)?;
    let updated = cli::support::scrub_env_file(&env_path)?;
    if updated == 0 {
        output::event(
            "env",
            LogLevel::Info,
            &format!("No sensitive values scrubbed in {}", env_path.display()),
            Persistence::Persistent,
        );
    } else {
        output::event(
            "env",
            LogLevel::Success,
            &format!(
                "Scrubbed {} sensitive values in {}",
                updated,
                env_path.display()
            ),
            Persistence::Persistent,
        );
    }
    Ok(())
}
