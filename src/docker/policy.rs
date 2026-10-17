//! Docker execution policy values and parsing.

use anyhow::Result;
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DockerPolicy {
    pub(crate) max_heavy_ops: usize,
    pub(crate) max_build_ops: usize,
    pub(crate) retry_budget: u32,
}

impl Default for DockerPolicy {
    fn default() -> Self {
        Self {
            max_heavy_ops: 2,
            max_build_ops: 1,
            retry_budget: 3,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DockerPolicyOverrides {
    pub(crate) max_heavy_ops: Option<usize>,
    pub(crate) max_build_ops: Option<usize>,
    pub(crate) retry_budget: Option<u32>,
}

static POLICY_OVERRIDES: OnceLock<Mutex<DockerPolicyOverrides>> = OnceLock::new();

pub(crate) fn set_policy_overrides(overrides: DockerPolicyOverrides) {
    let mut state = POLICY_OVERRIDES
        .get_or_init(|| Mutex::new(DockerPolicyOverrides::default()))
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    *state = overrides;
}

/// Returns docker execution policy from environment overrides or defaults.
pub(crate) fn docker_policy() -> DockerPolicy {
    let mut policy = policy_from_env(|key| {
        std::env::var_os(key).map(|value| value.to_string_lossy().to_string())
    });

    let overrides = *POLICY_OVERRIDES
        .get_or_init(|| Mutex::new(DockerPolicyOverrides::default()))
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    if let Some(value) = overrides.max_heavy_ops {
        policy.max_heavy_ops = value;
    }
    if let Some(value) = overrides.max_build_ops {
        policy.max_build_ops = value;
    }
    if let Some(value) = overrides.retry_budget {
        policy.retry_budget = value;
    }

    policy
}

fn policy_from_env<F>(lookup: F) -> DockerPolicy
where
    F: Fn(&str) -> Option<String>,
{
    let mut policy = DockerPolicy::default();
    if let Some(value) = lookup("BERTH_DOCKER_MAX_HEAVY_OPS")
        && let Ok(parsed) = parse_positive_u32("BERTH_DOCKER_MAX_HEAVY_OPS", &value)
    {
        policy.max_heavy_ops = parsed as usize;
    }
    if let Some(value) = lookup("BERTH_DOCKER_MAX_BUILD_OPS")
        && let Ok(parsed) = parse_positive_u32("BERTH_DOCKER_MAX_BUILD_OPS", &value)
    {
        policy.max_build_ops = parsed as usize;
    }
    if let Some(value) = lookup("BERTH_DOCKER_RETRY_BUDGET")
        && let Ok(parsed) = parse_positive_u32("BERTH_DOCKER_RETRY_BUDGET", &value)
    {
        policy.retry_budget = parsed;
    }
    policy
}

fn parse_positive_u32(name: &str, value: &str) -> Result<u32> {
    let parsed = value
        .trim()
        .parse::<u32>()
        .map_err(|err| anyhow::anyhow!("{name} must be a positive integer: {err}"))?;
    if parsed == 0 {
        anyhow::bail!("{name} must be >= 1");
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        DockerPolicy, DockerPolicyOverrides, docker_policy, parse_positive_u32, policy_from_env,
        set_policy_overrides,
    };

    #[test]
    fn parse_positive_u32_rejects_zero() {
        assert!(parse_positive_u32("value", "0").is_err());
    }

    #[test]
    fn docker_policy_defaults_are_safe_for_local_runs() {
        let policy = DockerPolicy::default();
        assert_eq!(policy.max_heavy_ops, 2);
        assert_eq!(policy.max_build_ops, 1);
        assert_eq!(policy.retry_budget, 3);
    }

    #[test]
    fn docker_policy_reads_valid_env_overrides() {
        let env = HashMap::from([
            ("BERTH_DOCKER_MAX_HEAVY_OPS".to_owned(), "4".to_owned()),
            ("BERTH_DOCKER_MAX_BUILD_OPS".to_owned(), "2".to_owned()),
            ("BERTH_DOCKER_RETRY_BUDGET".to_owned(), "5".to_owned()),
        ]);
        let policy = policy_from_env(|name| env.get(name).cloned());

        assert_eq!(policy.max_heavy_ops, 4);
        assert_eq!(policy.max_build_ops, 2);
        assert_eq!(policy.retry_budget, 5);
    }

    #[test]
    fn docker_policy_prefers_process_overrides() {
        set_policy_overrides(DockerPolicyOverrides {
            max_heavy_ops: Some(6),
            max_build_ops: Some(2),
            retry_budget: Some(9),
        });
        let policy = docker_policy();
        set_policy_overrides(DockerPolicyOverrides::default());

        assert_eq!(policy.max_heavy_ops, 6);
        assert_eq!(policy.max_build_ops, 2);
        assert_eq!(policy.retry_budget, 9);
    }
}
