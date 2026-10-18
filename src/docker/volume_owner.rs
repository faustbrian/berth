//! Which berth installation a named volume belongs to, by its label.

use anyhow::Result;

/// Whether `volume` exists and carries another berth installation's label
/// (another `BERTH_HOME` using the same project and instance names). An
/// unlabeled volume, or one that does not exist, is not foreign.
///
/// # Errors
///
/// Returns an error when Docker cannot be run or the installation ID cannot
/// be read.
pub(crate) fn is_foreign_volume(volume: &str) -> Result<bool> {
    let inspected = super::run_docker_output(
        &[
            "volume",
            "inspect",
            "--format",
            "{{index .Labels \"dev.berth.installation\"}}",
            volume,
        ],
        "failed to inspect a volume",
    )?;
    if !inspected.status.success() {
        return Ok(false);
    }
    let owner = String::from_utf8_lossy(&inspected.stdout).trim().to_owned();
    if owner.is_empty() || owner == "<no value>" {
        return Ok(false);
    }
    Ok(owner != crate::home::installation_id()?)
}
