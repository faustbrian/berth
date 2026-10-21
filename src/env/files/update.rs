//! env files update module.
//!
//! Contains env files update logic used by Berth command workflows.

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::mutations::{append_missing_values, apply_value_updates};
use super::{quote_env_value, read_env_lines, write_env_lines};
use crate::env::mapping::build_laravel_env_map;

/// Updates the .env file at `env_path` with values from service config.
///
/// Replaces existing environment variable lines in-place. With
/// `create_missing=true`, missing vars are appended.
///
/// # Errors
///
/// Returns an error if the .env file cannot be read or written.
pub(crate) fn update_env(
    service: &ServiceConfig,
    env_path: &Path,
    create_missing: bool,
) -> Result<()> {
    let mut lines = read_env_lines(env_path)?;
    let var_map = build_laravel_env_map(service);
    let mut updated = apply_value_updates(&mut lines, &var_map, quote_env_value);

    if create_missing {
        append_missing_values(&mut lines, &var_map, &mut updated, quote_env_value);
    }

    write_env_lines(env_path, lines)?;

    if updated.is_empty() {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("No matching variables found in {}", env_path.display()),
            Persistence::Persistent,
        );
    } else {
        for var in updated {
            output::event(
                &service.name,
                LogLevel::Success,
                &format!("Updated env variable {var}"),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::update_env;
    use std::path::PathBuf;

    #[test]
    fn update_env_escapes_special_characters() -> anyhow::Result<()> {
        crate::docker::with_dry_run_state(false, || -> anyhow::Result<()> {
            let env_path = unique_path("update");
            std::fs::write(&env_path, "DB_PASSWORD=\"old\"\n")?;

            let mut service = crate::config::preset_preview("mysql")?;
            service.password = Some(String::from("with\"quote\\slash\nline"));

            update_env(&service, &env_path, false)?;
            let content = std::fs::read_to_string(&env_path)?;

            assert_eq!(content, "DB_PASSWORD=\"with\\\"quote\\\\slash\\nline\"\n");
            Ok(())
        })
    }

    fn unique_path(suffix: &str) -> PathBuf {
        let nanos = crate::test_support::unique_stamp();
        crate::test_support::temp_root().join(format!("berth-env-update-{suffix}-{nanos}.env"))
    }

    #[test]
    fn update_env_does_not_touch_prefixed_keys() -> anyhow::Result<()> {
        crate::docker::with_dry_run_state(false, || -> anyhow::Result<()> {
            let env_path = unique_path("prefix");
            std::fs::write(&env_path, "DB_PASSWORD_EXTRA=\"keep\"\n")?;

            let mut service = crate::config::preset_preview("mysql")?;
            service.password = Some(String::from("new"));

            update_env(&service, &env_path, false)?;
            let content = std::fs::read_to_string(&env_path)?;

            assert_eq!(content, "DB_PASSWORD_EXTRA=\"keep\"\n");
            Ok(())
        })
    }
}
