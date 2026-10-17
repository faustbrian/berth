//! Child argument construction for swarm target invocations.

use crate::cli::args::PortStrategyArg;

/// Builds the complete argument vector passed to each child `berth` process.
///
/// This function injects project-root scoping and selected runtime flags, then
/// appends child command modifiers when not already present.
pub(super) fn swarm_child_args(
    target: &super::super::targets::ResolvedSwarmTarget,
    command: &[String],
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    env_output: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    runtime_env: Option<&str>,
) -> Vec<String> {
    let mut args = Vec::new();
    if quiet {
        args.push("--quiet".to_owned());
    }
    if no_color {
        args.push("--no-color".to_owned());
    }
    if dry_run {
        args.push("--dry-run".to_owned());
    }
    if let Some(env_name) = runtime_env {
        args.push("--env".to_owned());
        args.push(env_name.to_owned());
    }
    args.push("--project-root".to_owned());
    args.push(target.root.display().to_string());
    let mut child_command = command.to_vec();
    if should_enable_publish_all(&child_command) {
        child_command.push("--publish-all".to_owned());
    }
    if should_set_port_strategy(&child_command, port_strategy) {
        child_command.push("--port-strategy".to_owned());
        child_command.push("stable".to_owned());
    }
    if should_set_port_seed(&child_command, port_seed) {
        child_command.push("--port-seed".to_owned());
        child_command.push(port_seed.unwrap_or_default().to_owned());
    }
    if should_env_output(&child_command, env_output) {
        child_command.push("--env-output".to_owned());
    }
    if should_add_no_deps(&child_command) {
        child_command.push("--no-deps".to_owned());
    }
    args.extend(child_command);
    args
}

/// Enables `--publish-all` for `up`/`recreate` when not explicitly set.
fn should_enable_publish_all(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|subcommand| matches!(subcommand.as_str(), "up" | "recreate"))
        && !command.iter().any(|arg| arg == "--publish-all")
}

/// Enables deterministic `--port-strategy stable` for swarm `up` when requested.
fn should_set_port_strategy(command: &[String], port_strategy: PortStrategyArg) -> bool {
    port_strategy == PortStrategyArg::Stable && should_forward_up_flag(command, "--port-strategy")
}

/// Forwards `--port-seed` only when provided and absent in the child command.
fn should_set_port_seed(command: &[String], port_seed: Option<&str>) -> bool {
    port_seed.is_some() && should_forward_up_flag(command, "--port-seed")
}

/// Forwards `--env-output` to child `up` commands when enabled.
fn should_env_output(command: &[String], env_output: bool) -> bool {
    env_output && should_forward_up_flag(command, "--env-output")
}

/// Prevents child processes from re-resolving project dependencies because the
/// parent swarm invocation already owns dependency planning for the workspace.
fn should_add_no_deps(command: &[String]) -> bool {
    command.first().is_some_and(|subcommand| {
        matches!(subcommand.as_str(), "up" | "recreate" | "start" | "down")
    }) && !command.iter().any(|arg| arg == "--no-deps")
}

fn should_forward_up_flag(command: &[String], flag: &str) -> bool {
    command.first().is_some_and(|subcommand| subcommand == "up")
        && !command.iter().any(|arg| arg == flag)
}
