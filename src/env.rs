//! .env file updater for service variables.

#![allow(clippy::print_stdout)] // Env updates need to print status

mod dotenv_view;
mod files;
mod infer;
mod managed_runtime;
mod mapping;
#[cfg(test)]
mod tests;

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::{Config, ServiceConfig};

/// Updates env to persisted or external state.
pub fn update_env(service: &ServiceConfig, env_path: &Path, create_missing: bool) -> Result<()> {
    files::update_env(service, env_path, create_missing)
}

pub(crate) use dotenv_view::{host_env, render_dotenv};
pub(crate) use managed_runtime::apply_managed_runtime_env;

#[must_use]
pub fn inferred_app_env(config: &Config) -> HashMap<String, String> {
    infer::inferred_app_env(config)
}

/// Writes env values to persisted or external state.
pub fn write_env_values(
    env_path: &Path,
    values: &HashMap<String, String>,
    create_missing: bool,
) -> Result<()> {
    files::write_env_values(env_path, values, create_missing)
}

/// Merges environment values while preventing HTTPS URL downgrades for app URLs.
///
/// Incoming values still override by default, except when `APP_URL` or
/// `ASSET_URL` already has an `https://` value and the incoming override does
/// not.
pub fn merge_with_protected_https_app_urls(
    base: &mut HashMap<String, String>,
    incoming: &HashMap<String, String>,
) {
    for (key, value) in incoming {
        if should_block_insecure_app_url_override(base, key, value) {
            continue;
        }
        base.insert(key.clone(), value.clone());
    }
}

fn should_block_insecure_app_url_override(
    base: &HashMap<String, String>,
    key: &str,
    incoming_value: &str,
) -> bool {
    if !matches!(key, "APP_URL" | "ASSET_URL") {
        return false;
    }

    let Some(existing) = base.get(key) else {
        return false;
    };

    existing.starts_with("https://") && !incoming_value.starts_with("https://")
}
