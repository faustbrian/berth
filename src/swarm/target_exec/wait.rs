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
        // Polled before the stop flag: a target that already finished is
        // reported as finished, never as cancelled and run again.
        if let Some(done) = child
            .try_wait()
            .with_context(|| format!("failed polling swarm target '{}'", target.name))?
        {
            break done;
        }

        if let Some(cancel_flag) = &cancel
            && cancel_flag.load(Ordering::Relaxed)
        {
            drop(child.kill());
            let status = child.wait().with_context(|| {
                format!("failed waiting on cancelled swarm target '{}'", target.name)
            })?;
            if exited_on_its_own(status) {
                break status;
            }
            emit_target_cancelled(target, output_mode);
            return Ok(SwarmRunResult {
                name: target.name.clone(),
                root: target.root.clone(),
                success: false,
                skipped: true,
                exit_code: None,
            });
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    emit_target_completion(target, output_mode, status.success());

    Ok(SwarmRunResult {
        name: target.name.clone(),
        root: target.root.clone(),
        success: status.success(),
        skipped: false,
        exit_code: Some(crate::docker::exit_code_of(status)),
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

/// Whether a target the stop killed had in fact exited on its own just
/// before: then it has an exit code, while the kill (SIGKILL) leaves none.
fn exited_on_its_own(status: std::process::ExitStatus) -> bool {
    status.code().is_some()
}

#[cfg(test)]
mod tests {
    use super::wait_for_target_completion;
    use crate::swarm::target_exec::OutputMode;
    use crate::swarm::targets::ResolvedSwarmTarget;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    fn target() -> ResolvedSwarmTarget {
        ResolvedSwarmTarget {
            name: "api".to_owned(),
            root: "/tmp/api".into(),
        }
    }

    fn stopped() -> Option<Arc<AtomicBool>> {
        Some(Arc::new(AtomicBool::new(true)))
    }

    #[test]
    fn a_target_that_finished_before_the_stop_is_reported_as_finished() {
        for (script, code) in [("exit 0", 0), ("exit 3", 3)] {
            let mut child = std::process::Command::new("sh")
                .args(["-c", script])
                .spawn()
                .expect("spawn");
            // Block until it has exited, without reaping it, so the stop
            // flag is seen only after it finished on its own.
            let pid = rustix::process::Pid::from_child(&child);
            rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::EXITED | rustix::process::WaitIdOptions::NOWAIT,
            )
            .expect("waitid");
            let result =
                wait_for_target_completion(&mut child, &target(), OutputMode::Logged, stopped())
                    .expect("result");
            assert!(!result.skipped, "{script}");
            assert_eq!(result.exit_code, Some(code), "{script}");
        }
    }

    #[test]
    fn only_a_kill_counts_as_stopped() {
        use std::os::unix::process::ExitStatusExt;
        assert!(super::exited_on_its_own(
            std::process::ExitStatus::from_raw(0)
        ));
        assert!(super::exited_on_its_own(
            std::process::ExitStatus::from_raw(3 << 8)
        ));
        assert!(!super::exited_on_its_own(
            std::process::ExitStatus::from_raw(9)
        ));
    }

    #[test]
    fn a_running_target_is_stopped_and_reported_as_skipped() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn");
        let result =
            wait_for_target_completion(&mut child, &target(), OutputMode::Logged, stopped())
                .expect("result");
        assert!(result.skipped);
        assert_eq!(result.exit_code, None);
    }
}
