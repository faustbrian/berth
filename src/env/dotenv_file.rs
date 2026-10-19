//! Reads the project dotenv Laravel loads, without ever writing it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::ServiceConfig;

/// The `APP_ENV` an app's container receives: its own `env` comes after the
/// injected values in its `docker run` and wins (`container_env_pairs`).
pub(super) fn container_app_env<'a>(
    service: &'a ServiceConfig,
    vars: &'a HashMap<String, String>,
) -> Option<&'a String> {
    service
        .env
        .as_ref()
        .and_then(|env| env.get("APP_ENV"))
        .or_else(|| vars.get("APP_ENV"))
}

/// The dotenv file Laravel loads: `.env.{APP_ENV}` when `APP_ENV` is set
/// and that file exists (test runs use `.env.testing`), else `.env`.
pub(super) fn loaded_dotenv(root: &Path, app_env: Option<&String>) -> PathBuf {
    if let Some(name) = app_env.filter(|name| !name.is_empty()) {
        let specific = root.join(format!(".env.{name}"));
        if specific.is_file() {
            return specific;
        }
    }
    root.join(".env")
}

/// Whether a dotenv file sets `key` to a non-empty value, in any form
/// phpdotenv accepts (`export KEY=…`, spaces around `=`, quoted values).
/// A missing or unreadable file sets nothing; the last definition of the
/// key wins. Laravel's `Env::get` reads
/// `null`, `(null)`, `empty`, `(empty)`, `false` and `(false)` (in any case)
/// as null, `""` or `false`, so those set nothing either.
pub(super) fn dotenv_sets(path: &Path, key: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    // phpdotenv lets a later definition in the same file replace an earlier
    // one, so the last line that names the key decides.
    let Some(value) = contents.lines().rev().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        line.split_once('=')
            .filter(|(name, _)| name.trim() == key)
            .map(|(_, value)| value)
    }) else {
        return false;
    };
    let value = value
        .split(" #")
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(|quote| quote == '"' || quote == '\'')
        .to_ascii_lowercase();
    !matches!(
        value.as_str(),
        "" | "null" | "(null)" | "empty" | "(empty)" | "false" | "(false)"
    )
}
