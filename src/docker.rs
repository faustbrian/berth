//! Docker container management for service instances.

#![allow(clippy::print_stdout)] // Container operations need to print status
#![allow(clippy::items_after_statements)] // Local helper imports keep function scopes tight
#![allow(clippy::match_same_arms)] // Explicit driver branches document intent
#![allow(clippy::redundant_iter_cloned)] // Thread spawn paths need owned service values

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::config::ContainerEngine;
use crate::output::{self, LogLevel, Persistence};

static DRY_RUN: AtomicBool = AtomicBool::new(false);
static CONTAINER_ENGINE: OnceLock<Mutex<ContainerEngine>> = OnceLock::new();
#[cfg(test)]
static TEST_DOCKER_COMMAND: OnceLock<Mutex<Option<String>>> = OnceLock::new();
#[cfg(test)]
static TEST_RUNTIME_SCOPE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    /// Per-test engine override. Tests that only read the active engine never
    /// take the runtime scope lock, so a process-wide override would leak into
    /// concurrently running tests.
    static TEST_ENGINE_OVERRIDE: std::cell::Cell<Option<ContainerEngine>> =
        const { std::cell::Cell::new(None) };
}

/// Pull behavior used by `up`.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum PullPolicy {
    Always,
    Missing,
    Never,
}

/// Enable or disable dry-run mode.
pub fn set_dry_run(enabled: bool) {
    DRY_RUN.store(enabled, Ordering::SeqCst);
}

/// Returns true when dry-run mode is enabled.
#[must_use]
pub fn is_dry_run() -> bool {
    DRY_RUN.load(Ordering::SeqCst)
}

fn container_engine_lock() -> &'static Mutex<ContainerEngine> {
    CONTAINER_ENGINE.get_or_init(|| Mutex::new(ContainerEngine::default()))
}

fn active_engine_adapter() -> &'static dyn engine::RuntimeEngineAdapter {
    engine::adapter_for(container_engine())
}

pub(crate) fn set_container_engine(engine: ContainerEngine) {
    // Test builds keep engine selection per thread so one test's config
    // cannot change the engine observed by concurrently running tests.
    #[cfg(test)]
    TEST_ENGINE_OVERRIDE.set(Some(engine));
    #[cfg(not(test))]
    {
        let mut guard = container_engine_lock()
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        *guard = engine;
    }
}

#[must_use]
pub(crate) fn container_engine() -> ContainerEngine {
    #[cfg(test)]
    if let Some(engine) = TEST_ENGINE_OVERRIDE.get() {
        return engine;
    }
    *container_engine_lock()
        .lock()
        .unwrap_or_else(|err| err.into_inner())
}

#[must_use]
pub(crate) fn host_gateway_alias() -> &'static str {
    active_engine_adapter().host_gateway_alias()
}

#[must_use]
pub(crate) fn host_gateway_mapping() -> Option<&'static str> {
    active_engine_adapter().host_gateway_mapping()
}

#[must_use]
pub(crate) fn runtime_diagnostic_checks() -> &'static [RuntimeDiagnosticCheck] {
    active_engine_adapter().diagnostics()
}

#[must_use]
pub(crate) fn runtime_event_source_label() -> &'static str {
    active_engine_adapter().event_source_label()
}

#[must_use]
pub(crate) fn runtime_log_source_key() -> &'static str {
    active_engine_adapter().log_source_key()
}

#[cfg(test)]
pub(crate) fn with_dry_run_lock<R>(test: impl FnOnce() -> R) -> R {
    with_dry_run_state(true, test)
}

#[cfg(test)]
pub(crate) fn with_test_runtime_lock<R>(test: impl FnOnce() -> R) -> R {
    with_test_runtime_state(test)
}

#[cfg(test)]
pub(crate) fn with_container_engine<R>(engine: ContainerEngine, test: impl FnOnce() -> R) -> R {
    with_test_runtime_state(|| {
        let previous = TEST_ENGINE_OVERRIDE.replace(Some(engine));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));
        TEST_ENGINE_OVERRIDE.set(previous);

        match result {
            Ok(result) => result,
            Err(err) => std::panic::resume_unwind(err),
        }
    })
}

#[cfg(test)]
pub(crate) fn with_dry_run_state<R>(enabled: bool, test: impl FnOnce() -> R) -> R {
    with_test_runtime_state(|| {
        let previous = DRY_RUN.load(Ordering::SeqCst);
        DRY_RUN.store(enabled, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

        DRY_RUN.store(previous, Ordering::SeqCst);

        match result {
            Ok(result) => result,
            Err(err) => std::panic::resume_unwind(err),
        }
    })
}

pub(crate) fn print_docker_command(args: &[String]) {
    output::event(
        runtime_log_source_key(),
        LogLevel::Info,
        &format!("[dry-run] {}", runtime_command_text(args)),
        Persistence::Transient,
    );
}

#[must_use]
pub(crate) fn runtime_command_text(args: &[String]) -> String {
    format!(
        "{} {}",
        active_engine_adapter().command_binary(),
        args.join(" ")
    )
}

#[must_use]
pub(crate) fn runtime_command_error_context(action: &str) -> String {
    format!(
        "Failed to execute {} {action} command",
        active_engine_adapter().command_binary()
    )
}

#[cfg(test)]
pub(crate) fn with_docker_command<F, T>(command: &str, test: F) -> T
where
    F: FnOnce() -> T,
{
    with_test_runtime_state(|| {
        let previous = {
            let mut command_state = TEST_DOCKER_COMMAND
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            let previous = command_state.clone();
            *command_state = Some(command.to_owned());
            previous
        };

        let result = {
            let test = std::panic::AssertUnwindSafe(test);
            std::panic::catch_unwind(test)
        };

        {
            let mut command_state = TEST_DOCKER_COMMAND
                .get_or_init(Default::default)
                .lock()
                .unwrap_or_else(|err| err.into_inner());
            *command_state = previous;
        }
        match result {
            Ok(result) => result,
            Err(err) => std::panic::resume_unwind(err),
        }
    })
}

#[cfg(test)]
fn with_test_runtime_state<R>(test: impl FnOnce() -> R) -> R {
    std::thread_local! {
        static TEST_RUNTIME_SCOPE_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    let lock = TEST_RUNTIME_SCOPE_DEPTH.with(|depth| {
        let current = depth.get();
        depth.set(current.saturating_add(1));
        if current == 0 {
            Some(
                TEST_RUNTIME_SCOPE_LOCK
                    .get_or_init(Default::default)
                    .lock()
                    .unwrap_or_else(|err| err.into_inner()),
            )
        } else {
            None
        }
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    TEST_RUNTIME_SCOPE_DEPTH.with(|depth| {
        depth.set(depth.get().saturating_sub(1));
    });
    drop(lock);

    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

pub(crate) fn docker_command() -> String {
    #[cfg(test)]
    {
        let command_state = TEST_DOCKER_COMMAND
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some(command) = command_state.clone() {
            return command;
        }
    }
    active_engine_adapter().command_binary().to_owned()
}

mod child_exit;
mod cmd;
mod engine;
mod exec;
mod failure_message;
mod health;
mod image_inspect;
mod image_pull;
mod inspect;
mod labels;
mod logs;
mod manage;
mod ops;
mod policy;
mod scheduler;
mod up;

pub(crate) use child_exit::{
    ChildExitError, command_failed_error, exit_code_of, pass_through_exit_code,
};
pub(crate) use cmd::{
    docker_arg_refs, ensure_docker_output_success, run_docker_output, run_docker_output_owned,
    run_docker_status, run_docker_status_owned, spawn_docker_stdin_stderr_piped,
    spawn_docker_stdout_stderr_piped,
};
pub(crate) use engine::RuntimeDiagnosticCheck;
pub(crate) use exec::build_exec_args;
pub use exec::{exec_command, exec_interactive, exec_piped};
pub(crate) use failure_message::command_failed_in_container;
pub use health::wait_until_healthy;
pub(crate) use image_inspect::{docker_image_exists, docker_image_repo_digest};
pub(crate) use image_pull::docker_pull;
pub use inspect::{inspect_host_port_binding, inspect_json, inspect_label, inspect_status};
pub(crate) use labels::{
    ContainerRole, LABEL_CONTAINER, LABEL_MANAGED, VALUE_MANAGED_TRUE, ownership_label_args,
};
pub use logs::{LogsOptions, logs, logs_many, logs_prefixed};
pub use manage::{down, pull, recreate, restart, rm, stop};
pub use ops::{
    CpOptions, PruneOptions, StatsOptions, attach, cp, events, inspect_container, kill, pause,
    port, port_output, prune, prune_stopped_container, stats, top, unpause, wait,
};
pub(crate) use policy::{DockerPolicyOverrides, set_policy_overrides};
pub(crate) use scheduler::{DockerOpClass, with_scheduled_docker_op};
pub use up::up;

#[cfg(test)]
mod tests {
    use crate::config::ContainerEngine;
    use crate::docker::{host_gateway_alias, is_dry_run, with_dry_run_lock};

    #[test]
    fn with_dry_run_lock_sets_dry_run_inside_closure() {
        with_dry_run_lock(|| {
            assert!(is_dry_run());
        });
    }

    #[test]
    fn with_dry_run_lock_propagates_panics() {
        let result = std::panic::catch_unwind(|| {
            with_dry_run_lock(|| {
                panic!("simulated panic");
            })
        });

        assert!(result.is_err());
    }

    #[test]
    fn default_container_engine_uses_docker_binary_and_alias() {
        super::with_container_engine(ContainerEngine::Docker, || {
            assert_eq!(super::container_engine(), ContainerEngine::Docker);
            assert_eq!(super::docker_command(), "docker");
            assert_eq!(host_gateway_alias(), "host.docker.internal");
            assert_eq!(super::runtime_command_text(&["ps".to_owned()]), "docker ps");
            assert_eq!(
                super::runtime_command_error_context("logs"),
                "Failed to execute docker logs command"
            );
            assert_eq!(super::runtime_diagnostic_checks().len(), 2);
            assert_eq!(
                super::runtime_diagnostic_checks()[0].success_message,
                "Docker CLI available"
            );
            assert_eq!(super::runtime_event_source_label(), "Docker daemon");
            assert_eq!(super::runtime_log_source_key(), "docker");
        });
    }

    #[test]
    fn container_engine_switches_command_and_alias_to_podman() {
        super::with_container_engine(ContainerEngine::Podman, || {
            assert_eq!(super::container_engine(), ContainerEngine::Podman);
            assert_eq!(super::docker_command(), "podman");
            assert_eq!(host_gateway_alias(), "host.containers.internal");
            assert_eq!(super::runtime_command_text(&["ps".to_owned()]), "podman ps");
            assert_eq!(
                super::runtime_command_error_context("logs"),
                "Failed to execute podman logs command"
            );
            assert_eq!(super::runtime_diagnostic_checks().len(), 2);
            assert_eq!(
                super::runtime_diagnostic_checks()[0].success_message,
                "Podman CLI available"
            );
            assert_eq!(super::runtime_event_source_label(), "Podman runtime");
            assert_eq!(super::runtime_log_source_key(), "podman");
        });
    }
}
