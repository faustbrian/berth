//! Shared docker exec process runners.

use anyhow::{Context, Result};
use std::process::{Child, Command, ExitStatus, Stdio};

/// Runs `docker` with args and waits for completion.
pub(super) fn run_docker_status(args: &[String], error_context: &str) -> Result<ExitStatus> {
    crate::docker::run_docker_status_owned(args, error_context)
}

/// Spawns `docker` with piped stdin, stdout and stderr for a restore, which
/// relays stdout and reads berth's markers from it
/// (`database/restore/process.rs`).
pub(super) fn spawn_docker_piped(args: &[String]) -> Result<Child> {
    let arg_refs = crate::docker::docker_arg_refs(args);
    crate::docker::spawn_docker_stdin_stdout_stderr_piped(
        &arg_refs,
        &crate::docker::runtime_command_error_context("exec"),
    )
}

/// Spawns a placeholder child process for dry-run piping workflows.
pub(super) fn dry_run_process() -> Result<Child> {
    // It reads what a restore streams and discards it: a dry run never
    // prints the dump.
    Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn dry-run process")
}
