//! Whether a named volume exists.

use anyhow::Result;

/// Whether the engine has a volume named `volume`. Callers create a volume
/// only when it is missing: Podman refuses `volume create` for an existing
/// name, where Docker returns the existing volume.
///
/// # Errors
///
/// Returns an error when the engine cannot be run.
pub(crate) fn volume_exists(volume: &str) -> Result<bool> {
    let inspected = super::run_docker_output(
        &["volume", "inspect", "--format", "{{.Name}}", volume],
        "failed to inspect a volume",
    )?;
    Ok(inspected.status.success())
}
