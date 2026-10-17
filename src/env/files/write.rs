//! env files write module.
//!
//! Contains env files write logic used by Berth command workflows.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::mutations::{append_missing_values, apply_value_updates, purge_missing_managed_keys};
use super::quote_env_value;
use super::{read_env_lines, write_env_lines};
use crate::output::{self, LogLevel, Persistence};

/// Writes explicit environment values into an env file.
///
/// Existing keys are replaced; with `create_missing=true`, absent keys are appended.
///
/// # Errors
///
/// Returns an error if the env file cannot be read or written.
pub(crate) fn write_env_values(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
) -> Result<()> {
    write_env_values_with_purge(env_path, values, create_missing, &HashSet::new(), false)
}

/// Writes explicit environment values into an env file, optionally purging
/// stale managed keys that are no longer present in `values`.
///
/// Existing keys are replaced; with `create_missing=true`, absent keys are
/// appended.
///
/// # Errors
///
/// Returns an error if the env file cannot be read or written.
pub(crate) fn write_env_values_with_purge(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
    managed_keys: &HashSet<String>,
    purge_missing_managed: bool,
) -> Result<()> {
    if !env_path.exists() {
        anyhow::bail!("env file not found: {}", env_path.display());
    }

    let mut lines = read_env_lines(env_path)?;
    let mut updated = apply_value_updates(&mut lines, values, quote_env_value);

    if create_missing {
        append_missing_values(&mut lines, values, &mut updated, quote_env_value);
    }

    if purge_missing_managed {
        purge_missing_managed_keys(&mut lines, managed_keys, values);
    }

    write_env_lines(env_path, lines)?;

    if !updated.is_empty() {
        output::event(
            "env",
            LogLevel::Success,
            &format!(
                "Updated inferred app env values in {}: {}",
                env_path.display(),
                updated.join(", ")
            ),
            Persistence::Persistent,
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::write_env_values_with_purge;
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    #[test]
    fn write_env_values_with_purge_errors_when_env_file_is_missing() {
        let env_path = unique_path("missing");
        let values = HashMap::from([(String::from("APP_URL"), String::from("http://localhost"))]);
        let managed_keys = HashSet::from([String::from("APP_URL")]);

        let result = write_env_values_with_purge(&env_path, &values, true, &managed_keys, true);
        assert!(result.is_err());
    }

    fn unique_path(suffix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("berth-env-write-{suffix}-{nanos}.env"))
    }
}
