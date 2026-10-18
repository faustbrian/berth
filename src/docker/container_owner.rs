//! Refuses to act on a container another berth installation labeled.

use anyhow::Result;

/// Fails when container `name` exists and carries another berth
/// installation's label: another `BERTH_HOME` using the same project and
/// instance names derives the same container names, and stopping or
/// removing by name would act on its instance. A container that does not
/// exist, or carries no installation label, is not foreign. Checked before
/// every stop, kill, restart, start or removal by name.
///
/// # Errors
///
/// Returns an error naming the container and its installation, or when the
/// engine cannot be run or the installation ID cannot be read.
pub(crate) fn ensure_own_container(name: &str) -> Result<()> {
    // Unit tests run against fakes without labels and would read the
    // developer's own berth home; the integration tests cover this through
    // the binary (`tests/instance_leases.rs`).
    if cfg!(test) || super::is_dry_run() {
        return Ok(());
    }
    let inspected = super::run_docker_output(
        &[
            "inspect",
            "--format",
            "{{index .Config.Labels \"dev.berth.installation\"}}",
            name,
        ],
        "failed to inspect a container",
    )?;
    if !inspected.status.success() {
        return Ok(());
    }
    let owner = String::from_utf8_lossy(&inspected.stdout).trim().to_owned();
    if owner.is_empty() || owner == "<no value>" {
        return Ok(());
    }
    if owner != crate::home::installation_id()? {
        anyhow::bail!(
            "container {name} belongs to another berth installation ({owner}); berth leaves it \
             alone, use another --instance or that installation's BERTH_HOME"
        );
    }
    Ok(())
}
