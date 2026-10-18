//! Whether an existing shared server can join instance networks.

use anyhow::{Context, Result};

/// Whether `server` must be recreated before instances can attach to it:
/// on Podman, a server an earlier berth started without
/// [`crate::docker::attachable_network_args`] runs on rootless Podman's
/// private network stack, which `network connect` refuses. Such a container
/// reports no networks at all (`null`, Podman 5.7); a stopped bridge
/// container still lists its networks, so only the private stack matches.
/// Docker servers are always attachable.
///
/// # Errors
///
/// Returns an error when the server cannot be inspected.
pub(super) fn needs_attachable_recreation(server: &str) -> Result<bool> {
    if crate::docker::attachable_network_args().is_empty() {
        return Ok(false);
    }
    let output = crate::docker::run_docker_output(
        &[
            "inspect",
            "--format",
            "{{json .NetworkSettings.Networks}}",
            server,
        ],
        "failed to inspect the shared server",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to inspect {server}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let networks: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("unexpected network list for {server}"))?;
    Ok(networks.as_object().is_none_or(serde_json::Map::is_empty))
}
