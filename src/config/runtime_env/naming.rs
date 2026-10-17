//! config runtime env naming module.
//!
//! Contains config runtime env naming logic used by Berth command workflows.

use anyhow::Result;

/// Normalizes runtime env name into a canonical form.
pub(super) fn normalize_runtime_env_name(env_name: &str) -> Result<String> {
    let trimmed = env_name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("--env must not be empty");
    }

    let mut normalized = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            normalized.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') {
            normalized.push(ch);
        } else {
            normalized.push('-');
        }
    }

    let cleaned = normalized
        .trim_matches(|ch| matches!(ch, '-' | '_' | '.'))
        .to_owned();

    if cleaned.is_empty() {
        anyhow::bail!(
            "--env '{env_name}' is invalid after normalization; \
             use at least one alphanumeric character"
        );
    }

    // `{network}_ep` names a swarm source's endpoint network (L205); an
    // environment ending the same way would take that name.
    if cleaned.ends_with("_ep") {
        anyhow::bail!("--env '{env_name}' must not end in `_ep`, which berth reserves");
    }

    if cleaned == "test" {
        return Ok("testing".to_owned());
    }

    Ok(cleaned)
}

/// Returns whether the runtime env name is the default profile.
pub(super) fn is_default_runtime_env(env_name: &str) -> bool {
    matches!(env_name, "default" | "local")
}

#[cfg(test)]
mod reserved_suffix_tests {
    use super::normalize_runtime_env_name;

    #[test]
    fn the_endpoint_network_suffix_is_reserved() {
        assert!(normalize_runtime_env_name("foo_ep").is_err());
        assert_eq!(normalize_runtime_env_name("ep").expect("plain ep"), "ep");
        assert_eq!(
            normalize_runtime_env_name("staging").expect("staging"),
            "staging"
        );
    }
}
