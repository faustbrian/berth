//! swarm target exec module.
//!
//! Contains swarm target exec logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::targets::ResolvedSwarmTarget;
use crate::output::{self, LogLevel, Persistence};
use io::spawn_prefixed_output_threads;
use wait::wait_for_target_completion;

mod io;
mod wait;

const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy)]
pub(super) enum OutputMode {
    Logged,
    Passthrough,
}

#[derive(Debug)]
pub(super) struct SwarmRunResult {
    pub(super) name: String,
    pub(super) root: PathBuf,
    pub(super) success: bool,
    pub(super) skipped: bool,
}

/// Executes the requested command for one swarm target.
pub(super) fn run_swarm_target(
    berth_executable: &Path,
    target: &ResolvedSwarmTarget,
    args: &[String],
    output_mode: OutputMode,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<SwarmRunResult> {
    let mut command = Command::new(berth_executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if matches!(output_mode, OutputMode::Passthrough) && !args.iter().any(|arg| arg == "--no-color")
    {
        command.env("CLICOLOR_FORCE", "1");
        command.env_remove("NO_COLOR");
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run swarm target '{}'", target.name))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stdout pipe for '{}'", target.name))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stderr pipe for '{}'", target.name))?;

    let (stdout_thread, stderr_thread) =
        spawn_prefixed_output_threads(&target.name, stdout, stderr, output_mode);

    let result = wait_for_target_completion(&mut child, target, output_mode, cancel)?;

    drain_output_threads(stdout_thread, stderr_thread, target, output_mode);

    Ok(result)
}

fn drain_output_threads(
    stdout_thread: std::thread::JoinHandle<()>,
    stderr_thread: std::thread::JoinHandle<()>,
    target: &ResolvedSwarmTarget,
    output_mode: OutputMode,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let total = 2usize;
    for thread in [stdout_thread, stderr_thread] {
        let tx = tx.clone();
        std::thread::spawn(move || {
            drop(thread.join());
            let _ = tx.send(());
        });
    }
    drop(tx);

    let deadline = Instant::now() + OUTPUT_DRAIN_TIMEOUT;
    let mut drained = 0usize;
    while drained < total {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }

        match rx.recv_timeout(remaining) {
            Ok(()) => drained += 1,
            Err(_) => break,
        }
    }

    if drained < total {
        emit_output_drain_timeout(target, output_mode);
    }
}

fn emit_output_drain_timeout(target: &ResolvedSwarmTarget, output_mode: OutputMode) {
    let message = "Timed out draining target output; continuing";
    match output_mode {
        OutputMode::Logged => {
            output::event(
                &target.name,
                LogLevel::Warn,
                message,
                Persistence::Persistent,
            );
        }
        OutputMode::Passthrough => eprintln!("Target '{}' {message}", target.name),
    }
}
