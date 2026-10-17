//! Derived runtime image build helpers for serve targets.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::javascript::{JavaScriptRuntime, VersionManager};
use crate::serve::sql_client_flavor::SqlClientFlavor;

mod build;
mod docker_cmd;
mod dockerfile;
mod inspect;

/// Filters extension list to packages not already installed in base image.
pub(super) fn filter_installable_extensions(
    base_image: &str,
    extensions: &[String],
) -> Result<Vec<String>> {
    let preinstalled = inspect::installed_php_modules(base_image)?;
    Ok(extensions
        .iter()
        .filter(|ext| !preinstalled.contains(&ext.to_lowercase()))
        .cloned()
        .collect())
}

/// Returns installed PHP modules for a runtime image.
pub(super) fn installed_php_modules(base_image: &str) -> Result<std::collections::HashSet<String>> {
    inspect::installed_php_modules(base_image)
}

/// Returns whether JavaScript/composer tooling should be included in derived image.
pub(super) fn should_include_js_tooling(target: &ServiceConfig) -> bool {
    target.driver == crate::config::Driver::Frankenphp
}

/// Renders the Dockerfile used to build a derived serve runtime image.
#[expect(
    clippy::too_many_arguments,
    reason = "7.0.0 signature kept as is; it predates the 9.x lint baseline (L154)"
)]
pub(super) fn render_derived_dockerfile(
    base_image: &str,
    extensions: &[String],
    include_js_tooling: bool,
    runtime: JavaScriptRuntime,
    version_manager: VersionManager,
    node_version: Option<&str>,
    sql_client_flavor: SqlClientFlavor,
    playwright_package_spec: Option<&str>,
) -> String {
    dockerfile::render_derived_dockerfile(
        base_image,
        extensions,
        include_js_tooling,
        runtime,
        version_manager,
        node_version,
        sql_client_flavor,
        playwright_package_spec,
    )
}

/// Builds a derived serve image from rendered Dockerfile content.
pub(super) fn build_derived_image(tag: &str, dockerfile: &str) -> Result<()> {
    build::build_derived_image(tag, dockerfile)
}
