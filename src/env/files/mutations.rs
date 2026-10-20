//! Line mutation helpers for env file operations.

use std::collections::HashMap;

use super::parse::env_key;

pub(super) fn apply_value_updates<F>(
    lines: &mut [String],
    values: &HashMap<String, String>,
    mut quote_env_value: F,
) -> Vec<String>
where
    F: FnMut(&str) -> String,
{
    let mut updated = Vec::new();

    for line in lines {
        let Some(key) = env_key(line).map(ToOwned::to_owned) else {
            continue;
        };

        if let Some(value) = values.get(&key) {
            *line = format!("{key}={}", quote_env_value(value));
            updated.push(key);
        }
    }

    updated
}

pub(super) fn append_missing_values<F>(
    lines: &mut Vec<String>,
    values: &HashMap<String, String>,
    updated: &mut Vec<String>,
    mut quote_env_value: F,
) where
    F: FnMut(&str) -> String,
{
    for (key, value) in values {
        if !updated.iter().any(|existing| existing == key) {
            lines.push(format!("{key}={}", quote_env_value(value)));
            updated.push(key.clone());
        }
    }
}
