//! docker up state module.
//!
//! Contains docker up state logic used by Berth command workflows.

use anyhow::Result;

use crate::config::ServiceConfig;

use super::super::PullPolicy;
use super::{inspect_image_exists, remove_container, start_container};

/// Ensures or start existing exists and is in the required state.
pub(super) fn ensure_or_start_existing(container_name: &str, recreate: bool) -> Result<bool> {
    if recreate {
        remove_container(container_name);
        return Ok(false);
    }

    start_container(container_name)
}

/// Ensures image available exists and is in the required state.
pub(super) fn ensure_image_available(
    service: &ServiceConfig,
    pull_policy: PullPolicy,
) -> Result<()> {
    match pull_policy {
        PullPolicy::Always => super::super::pull(service)?,
        PullPolicy::Missing => {
            if !inspect_image_exists(&service.image)? {
                super::super::pull(service)?;
            }
        }
        PullPolicy::Never => {}
    }

    Ok(())
}
