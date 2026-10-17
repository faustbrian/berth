//! Container lifecycle boundary for serve targets.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

mod lifecycle;
pub(crate) use lifecycle::container_env_pairs;
mod run_config;

/// Ensures the serve container is running for a target.
pub(super) fn ensure_container_running(
    target: &ServiceConfig,
    recreate: bool,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    lifecycle::ensure_container_running(target, recreate, runtime_image, project_root, injected_env)
}

/// Resolves an explicit or derived container command for `docker run`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn resolved_run_command(target: &ServiceConfig) -> Option<Vec<String>> {
    run_config::resolved_run_command(target)
}

/// Resolves a volume mapping into an absolute host-path mapping when needed.
pub(super) fn resolve_volume_mapping(volume: &str, project_root: &Path) -> Result<String> {
    run_config::resolve_volume_mapping(volume, project_root)
}

/// Stops and removes a serve container for the target.
pub(super) fn remove_container(target: &ServiceConfig) -> Result<()> {
    lifecycle::remove_container(target)
}
