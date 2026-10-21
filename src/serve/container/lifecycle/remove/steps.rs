//! Docker step execution helpers for serve container removal.

use anyhow::Result;

use super::super::docker_cmd::docker_output;

#[derive(Clone, Copy)]
pub(super) enum StepOutcome {
    Performed,
    Missing,
}

pub(super) fn run_docker_step(
    args: [&str; 2],
    action: &str,
    container_name: &str,
) -> Result<StepOutcome> {
    let output = docker_output(&args, &crate::docker::runtime_command_error_context(action))?;

    if output.status.success() {
        return Ok(StepOutcome::Performed);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if crate::docker::reports_missing_container(&stderr) {
        return Ok(StepOutcome::Missing);
    }

    anyhow::bail!("failed to {action} serve container '{container_name}': {stderr}");
}
