//! cli support default env path module.
//!
//! Contains cli support default env path logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::config;

/// Returns the default value for env path.
pub(crate) fn default_env_path(
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    env_file: Option<&Path>,
    runtime_env: Option<&str>,
) -> Result<PathBuf> {
    if let Some(path) = env_file {
        return Ok(path.to_path_buf());
    }

    let root = super::workspace_root(config_path, project_root)?;

    let env_file_name = config::default_env_file_name(runtime_env)?;
    Ok(root.join(env_file_name))
}

#[cfg(test)]
mod tests {
    use super::default_env_path;
    use std::path::Path;

    #[test]
    fn default_env_path_uses_explicit_env_file() {
        assert_eq!(
            default_env_path(
                Some(Path::new("/tmp/custom/.env")),
                None,
                Some(Path::new("/tmp/custom/env.override")),
                None,
            )
            .expect("default env path"),
            Path::new("/tmp/custom/env.override")
        );
    }
}
