//! env files parse module.
//!
//! Contains env files parse logic used by Berth command workflows.

pub(super) fn env_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    trimmed.split_once('=').map(|(key, _)| key.trim())
}
