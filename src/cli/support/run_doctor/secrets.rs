//! cli support run doctor secrets module.
//!
//! Contains cli support run doctor secrets logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use super::report;

use super::super::find_sensitive_env_values::find_sensitive_env_values;

/// Checks sensitive env values and reports actionable failures.
pub(super) fn check_sensitive_env_values(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<bool> {
    if let Ok(root) = super::super::workspace_root(config_path, project_root) {
        let env_path = root.join(".env");
        if !env_path.exists() {
            return Ok(false);
        }

        let sensitive = find_sensitive_env_values(&env_path)?;
        if !sensitive.is_empty() {
            report::error(&format!(
                "Potential real secrets in {}: {}",
                env_path.display(),
                sensitive.join(", ")
            ));
            report::info(&format!(
                "Run: berth env-scrub --env-file {}",
                env_path.display()
            ));
            return Ok(true);
        }
    }

    Ok(false)
}
