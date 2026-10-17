//! Planning phase for swarm execution.
//!
//! These helpers validate CLI intent and turn config + workspace context into the
//! exact ordered target list to execute.

use anyhow::Result;
use std::path::Path;

use crate::swarm::targets::{
    ResolvedSwarmTarget, bootstrap_swarm_targets, enforce_shared_down_dependency_guard,
    ensure_swarm_target_configs_exist, resolve_swarm_targets,
};

pub(super) fn validate_swarm_invocation<'a>(
    command: &'a [String],
    parallel: usize,
) -> Result<&'a str> {
    crate::parallel::validate_parallelism(parallel)?;

    let Some(subcommand) = command.first() else {
        anyhow::bail!("missing swarm command");
    };
    if subcommand == "swarm" {
        anyhow::bail!("nested `berth swarm` is not supported");
    }

    Ok(subcommand)
}

/// Handles `swarm ls` shortcut mode.
///
/// Returns `Ok(true)` when listing was emitted and no execution should occur.
pub(super) fn handle_ls_command(
    config: &crate::config::Config,
    workspace_root: &Path,
    only: &[String],
    subcommand: &str,
    command_len: usize,
) -> Result<bool> {
    if subcommand == "ls" && command_len == 1 {
        for target in resolve_swarm_targets(config, workspace_root, only, false)? {
            println!("{}\t{}", target.name, target.root.display());
        }
        return Ok(true);
    }

    Ok(false)
}

/// Resolves and orders the final execution target list for a swarm run.
///
/// Important ordering rule:
/// - `swarm down` with dependencies reverses target order so dependents are
///   stopped before the services they rely on.
pub(super) fn resolve_execution_targets(
    config: &crate::config::Config,
    workspace_root: &Path,
    command: &[String],
    only: &[String],
    include_deps: bool,
    force_down_deps: bool,
    subcommand: &str,
    quiet: bool,
) -> Result<Vec<ResolvedSwarmTarget>> {
    if command.first().is_some_and(|sub| sub == "logs") && command.iter().any(|arg| arg == "--tui")
    {
        anyhow::bail!("`berth swarm logs --tui` was removed. Use `berth swarm logs` instead.");
    }

    let mut targets = resolve_swarm_targets(config, workspace_root, only, include_deps)?;
    if subcommand == "up" {
        bootstrap_swarm_targets(config, &targets, quiet)?;
    }
    ensure_swarm_target_configs_exist(&targets)?;

    if include_deps && subcommand == "down" {
        enforce_shared_down_dependency_guard(
            config,
            only,
            &targets,
            force_down_deps,
            workspace_root,
        )?;
        targets.reverse();
    }

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::{handle_ls_command, resolve_execution_targets, validate_swarm_invocation};
    use crate::config::{Config, SwarmTarget};

    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_workspace() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("berth-swarm-planning-{nanos}"));
        std::fs::create_dir_all(&path).expect("create workspace");
        path
    }

    fn target(name: &str) -> SwarmTarget {
        let root = std::env::temp_dir().join(format!("berth-swarm-target-{name}"));
        std::fs::create_dir_all(&root).expect("create target root");
        std::fs::write(root.join(".berth.toml"), "container_prefix = \"test\"\n")
            .expect("write target config");
        SwarmTarget {
            name: name.to_owned(),
            root,
            depends_on: Vec::new(),
            inject_env: Vec::new(),
            git: None,
        }
    }

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: vec![target("alpha"), target("beta"), target("gamma")],
        }
    }

    #[test]
    fn validate_swarm_invocation_rejects_empty_and_nested() {
        assert!(
            validate_swarm_invocation(&[], 1)
                .is_err_and(|err| err.to_string().contains("missing swarm command"))
        );
        assert!(validate_swarm_invocation(&["swarm".to_owned()], 1).is_err());
    }

    #[test]
    fn resolve_execution_targets_lists_by_subcommand() {
        let cfg = config();
        let workspace = temp_workspace();
        let command = vec!["up".to_owned()];
        let targets = resolve_execution_targets(
            &cfg,
            &workspace,
            &command,
            &Vec::new(),
            false,
            false,
            "up",
            false,
        )
        .expect("resolve up targets");
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].name, "alpha");
        assert_eq!(targets[1].name, "beta");
        assert_eq!(targets[2].name, "gamma");
    }

    #[test]
    fn resolve_execution_targets_reverses_down_targets_when_dependencies_included() {
        let mut cfg = config();
        cfg.swarm = vec![
            SwarmTarget {
                name: "alpha".to_owned(),
                root: target("alpha").root,
                depends_on: vec!["beta".to_owned()],
                inject_env: Vec::new(),
                git: None,
            },
            SwarmTarget {
                name: "beta".to_owned(),
                root: target("beta").root,
                depends_on: Vec::new(),
                inject_env: Vec::new(),
                git: None,
            },
        ];
        let workspace = temp_workspace();
        let command = vec!["down".to_owned()];
        let targets = resolve_execution_targets(
            &cfg,
            &workspace,
            &command,
            &Vec::new(),
            true,
            false,
            "down",
            false,
        )
        .expect("resolve down targets");
        assert_eq!(targets[0].name, "alpha");
        assert_eq!(targets[1].name, "beta");
    }

    #[test]
    fn handle_ls_command_emits_when_requested() -> anyhow::Result<()> {
        let cfg = config();
        let workspace = temp_workspace();
        let command = vec!["ls".to_owned()];
        assert!(handle_ls_command(
            &cfg,
            &workspace,
            &Vec::new(),
            "ls",
            command.len()
        )?);
        assert!(!handle_ls_command(
            &cfg,
            &workspace,
            &Vec::new(),
            "ls",
            command.len() + 1
        )?);
        Ok(())
    }

    #[test]
    fn validate_swarm_invocation_rejects_tui_logs() {
        let cfg = config();
        let workspace = temp_workspace();
        let command = vec!["logs".to_owned(), "--tui".to_owned()];
        let result = resolve_execution_targets(
            &cfg,
            &workspace,
            &command,
            &Vec::new(),
            false,
            false,
            "logs",
            false,
        );
        assert!(result.is_err());
        assert!(
            result
                .err()
                .is_some_and(|err| err.to_string().contains("removed"))
        );
    }
}
