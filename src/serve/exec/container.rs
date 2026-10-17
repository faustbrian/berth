//! `docker exec` command execution helpers for serve containers.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::docker::build_exec_args;
use crate::docker::command_failed_in_container;

/// Runs berth's own setup (compatibility wrappers under `/usr/local/bin`) as
/// root: app containers otherwise run as the invoking user (L023), who
/// cannot write there. Root gets no capability beyond the container's own
/// set (`NET_BIND_SERVICE`).
///
/// # Errors
///
/// Returns an error when the command fails.
pub(super) fn exec_setup_as_root(target: &ServiceConfig, command: &[String]) -> Result<()> {
    let container_name = target.container_name()?;
    let mut args = build_exec_args(&container_name, command, false);
    args.splice(1..1, ["--user".to_owned(), "0".to_owned()]);
    super::docker_cmd::run_or_log_docker(
        &target.name,
        &args,
        "failed to prepare the serve container",
        |exit_code| command_failed_in_container(&container_name, exit_code),
    )
}

/// Executes a command inside the app container.
///
/// # Errors
///
/// Returns an error if command execution fails.
pub(super) fn exec_command(target: &ServiceConfig, command: &[String], tty: bool) -> Result<()> {
    if command.is_empty() {
        anyhow::bail!("no command specified");
    }

    let container_name = target.container_name()?;
    let args = build_exec_args(&container_name, command, tty);

    super::docker_cmd::run_or_log_docker(
        &target.name,
        &args,
        "failed to execute artisan command in serve container",
        |exit_code| command_failed_in_container(&container_name, exit_code),
    )
}
