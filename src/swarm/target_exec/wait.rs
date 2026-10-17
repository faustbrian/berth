//! swarm target exec wait module.
//!
//! Contains swarm target exec wait logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::output::{self, LogLevel, Persistence};
use crate::swarm::targets::ResolvedSwarmTarget;

use super::{OutputMode, SwarmRunResult};

/// Waits for for target completion to reach a ready state.
pub(super) fn wait_for_target_completion(
    child: &mut Child,
    target: &ResolvedSwarmTarget,
    output_mode: OutputMode,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<SwarmRunResult> {
    let status = loop {
        if let Some(cancel_flag) = &cancel
            && cancel_flag.load(Ordering::Relaxed)
        {
            drop(child.kill());
            let _ = child.wait().with_context(|| {
                format!("failed waiting on cancelled swarm target '{}'", target.name)
            })?;
            emit_target_cancelled(target, output_mode);
            return Ok(SwarmRunResult {
                name: target.name.clone(),
                root: target.root.clone(),
                success: false,
                skipped: true,
            });
        }

        if let Some(done) = child
            .try_wait()
            .with_context(|| format!("failed polling swarm target '{}'", target.name))?
        {
            break done;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    emit_target_completion(target, output_mode, status.success());

    Ok(SwarmRunResult {
        name: target.name.clone(),
        root: target.root.clone(),
        success: status.success(),
        skipped: false,
    })
}

fn emit_target_cancelled(target: &ResolvedSwarmTarget, output_mode: OutputMode) {
    match output_mode {
        OutputMode::Logged => output::event(
            &target.name,
            LogLevel::Warn,
            "Target cancelled due to fail-fast policy",
            Persistence::Persistent,
        ),
        OutputMode::Passthrough => {
            eprintln!("Target '{}' cancelled due to fail-fast policy", target.name);
        }
    }
}

fn emit_target_completion(target: &ResolvedSwarmTarget, output_mode: OutputMode, success: bool) {
    match output_mode {
        OutputMode::Logged => emit_logged_completion(target, success),
        OutputMode::Passthrough => {
            if !success {
                eprintln!("Target '{}' failed", target.name);
            }
        }
    }
}

fn emit_logged_completion(target: &ResolvedSwarmTarget, success: bool) {
    let (level, message) = if success {
        (LogLevel::Success, "Target completed successfully")
    } else {
        (LogLevel::Error, "Target failed")
    };
    output::event(&target.name, level, message, Persistence::Persistent);
}
