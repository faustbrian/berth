//! shared env-output path resolution helpers.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub(crate) fn env_output_path(
    env_output: bool,
    _config_path: Option<&Path>,
    _project_root: Option<&Path>,
    _runtime_env: Option<&str>,
) -> Result<Option<PathBuf>> {
    if !env_output {
        return Ok(None);
    }
    anyhow::bail!("{}", env_write_removed())
}

/// Why berth refuses to write `.env`: the environment is injected into every
/// container, so a written copy only drifts (and leaks credentials into a
/// file agents commit by mistake).
pub(crate) fn env_write_removed() -> String {
    format!(
        "berth never writes .env: the environment is injected into every container. Print it \
         with {} (or {}), or write a new file with {}",
        crate::followup::command("env"),
        crate::followup::command("env --json"),
        crate::followup::command("env generate --output <file>")
    )
}
