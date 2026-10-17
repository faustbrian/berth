//! cli handlers env cmd generate module.
//!
//! Contains cli handlers env cmd generate logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{config, env};

/// Handles the `generate env` CLI command.
pub(super) fn handle_generate_env(
    config: &config::Config,
    output: &Path,
    quiet: bool,
) -> Result<()> {
    let values = env::managed_app_env(config);
    env::write_env_values_full(output, &values)?;
    log::info_if_not_quiet(
        quiet,
        "env",
        &format!("Generated {} with {} keys", output.display(), values.len()),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::docker;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config;

    use super::handle_generate_env;

    #[test]
    fn handle_generate_env_writes_all_values_to_destination() -> anyhow::Result<()> {
        let laravel = config::preset_preview("laravel")?;
        let mut mysql = config::preset_preview("mysql")?;
        mysql.name = "db".to_owned();

        let config_root = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Project,
            container_prefix: Some("berth".to_owned()),
            domain_strategy: None,
            service: vec![laravel, mysql],
            swarm: Vec::new(),
        };

        let output_path = PathBuf::from(format!(
            "/tmp/berth-generate-env-{}.env",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        if output_path.exists() {
            std::fs::remove_file(&output_path)?;
        }
        std::fs::write(&output_path, "").expect("seed env file");

        let result = docker::with_dry_run_state(false, || {
            handle_generate_env(&config_root, &output_path, true)
        });
        result?;
        let content = std::fs::read_to_string(&output_path)?;
        assert!(content.contains("BERTH_SQL_CLIENT_FLAVOR"));
        Ok(())
    }
}
