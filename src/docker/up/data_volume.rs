//! Labeled data volumes for dedicated services (ledger L039).
//!
//! `docker run -v name:/path` creates a missing volume without labels, so a
//! leaked one would be invisible to label-based checks and to `gc`. berth
//! creates each named volume first, with the ownership labels its containers
//! carry.

use anyhow::Result;

use crate::config::ServiceConfig;

/// Creates the service's missing named volumes with ownership labels.
/// Existing volumes (an earlier `up`, or one an older berth created) are
/// used as they are.
///
/// # Errors
///
/// Returns an error when Docker cannot inspect or create a volume.
pub(super) fn ensure_labeled_volumes(service: &ServiceConfig, container_name: &str) -> Result<()> {
    for volume in super::args_builder::named_volumes(service, container_name) {
        // Never adopt another installation's data (another `BERTH_HOME` with
        // the same project and instance names).
        if crate::docker::is_foreign_volume(&volume)? {
            anyhow::bail!(
                "volume {volume} belongs to another berth installation; use another --instance"
            );
        }
        if crate::docker::volume_exists(&volume)? {
            continue;
        }
        let mut args = vec!["volume".to_owned(), "create".to_owned()];
        args.extend(crate::docker::ownership_label_args(
            service,
            container_name,
            crate::docker::ContainerRole::Volume,
        )?);
        args.push(volume.clone());
        let created =
            crate::docker::run_docker_output_owned(&args, "failed to create a data volume")?;
        if !created.status.success() {
            anyhow::bail!(
                "failed to create volume {volume}: {}",
                String::from_utf8_lossy(&created.stderr).trim()
            );
        }
    }
    Ok(())
}
