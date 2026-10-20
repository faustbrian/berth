//! Lifecycle hook runner loop.

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::{HookOnError, HookPhase, ServiceConfig, ServiceHook};
use crate::output::{self, LogLevel, Persistence};

use super::hook_phase_label;

pub(super) fn run_phase_hooks_with_executor<F>(
    services: &[&ServiceConfig],
    phase: HookPhase,
    workspace_root: &Path,
    quiet: bool,
    mut executor: F,
) -> Result<()>
where
    F: FnMut(&ServiceConfig, &ServiceHook, HookPhase, &Path) -> Result<()>,
{
    for service in services {
        for hook in &service.hook {
            if hook.phase != phase {
                continue;
            }

            if !quiet {
                emit_hook_start(&service.name, &hook.name, phase);
            }

            let result = executor(service, hook, phase, workspace_root).with_context(|| {
                format!("hook '{}' failed for service '{}'", hook.name, service.name)
            });

            match result {
                Ok(()) => {
                    if !quiet {
                        emit_hook_success(&service.name, &hook.name);
                    }
                }
                Err(error) => {
                    if hook.on_error == HookOnError::Warn {
                        emit_hook_warn_continue(&service.name, &hook.name, &error.to_string());
                        continue;
                    }
                    // Only a hook that stops the command gets this advice; a
                    // `warn` hook already keeps going.
                    return Err(error.context(format!(
                        "fix hook '{}', or set `on_error: warn` on it to keep going past a failure",
                        hook.name
                    )));
                }
            }
        }
    }

    Ok(())
}

fn emit_hook_start(service_name: &str, hook_name: &str, phase: HookPhase) {
    output::event(
        service_name,
        LogLevel::Info,
        &format!("Running hook '{}' ({})", hook_name, hook_phase_label(phase)),
        Persistence::Persistent,
    );
}

fn emit_hook_success(service_name: &str, hook_name: &str) {
    output::event(
        service_name,
        LogLevel::Success,
        &format!("Hook '{}' completed", hook_name),
        Persistence::Persistent,
    );
}

fn emit_hook_warn_continue(service_name: &str, hook_name: &str, error: &str) {
    output::event(
        service_name,
        LogLevel::Warn,
        &format!(
            "Hook '{}' failed but continuing (`on_error=warn`): {error}",
            hook_name
        ),
        Persistence::Persistent,
    );
}
