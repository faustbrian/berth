//! Runtime image resolution facade for serve targets.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

mod browser;
mod derived;
mod lock;
mod runtime;
pub(crate) use browser::enable_browser_test_runtime;

/// Resolves the effective runtime image for a serve target.
pub(super) fn resolve_runtime_image(
    target: &ServiceConfig,
    allow_rebuild: bool,
    injected_env: &HashMap<String, String>,
    workspace_root: &Path,
) -> Result<String> {
    derived::resolve_runtime_image(target, allow_rebuild, injected_env, workspace_root)
}

/// Normalizes php extensions into a canonical form.
pub(super) fn normalize_php_extensions(extensions: &[String]) -> Vec<String> {
    runtime::normalize_php_extensions(extensions)
}

/// Returns whether FrankenPHP `SERVER_NAME` should be injected automatically.
pub(super) fn should_inject_frankenphp_server_name(
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) -> bool {
    runtime::should_inject_frankenphp_server_name(target, injected_env)
}

/// Returns Mailhog SMTP publish port for side-channel SMTP exposure.
pub(super) fn mailhog_smtp_port(target: &ServiceConfig) -> Option<u16> {
    runtime::mailhog_smtp_port(target)
}

#[cfg(test)]
pub(super) fn derived_image_tag(signature: &str) -> String {
    derived::derived_image_tag(signature)
}

#[cfg(test)]
pub(super) use super::image_build::render_derived_dockerfile;
