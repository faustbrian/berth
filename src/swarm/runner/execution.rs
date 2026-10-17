//! Swarm target execution engine.
//!
//! Supports sequential and parallel execution while preserving fail-fast semantics.

use anyhow::Result;
use std::path::Path;

use super::super::target_exec::{OutputMode, SwarmRunResult};
use crate::cli::args::PortStrategyArg;
use crate::swarm::targets::ResolvedSwarmTarget;

mod parallel;

pub(super) struct RunTargetsOptions<'a, F> {
    pub(super) targets: &'a [ResolvedSwarmTarget],
    pub(super) command: &'a [String],
    pub(super) output_mode: OutputMode,
    pub(super) parallel: usize,
    pub(super) fail_fast: bool,
    pub(super) port_strategy: PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    pub(super) env_output: bool,
    pub(super) quiet: bool,
    pub(super) no_color: bool,
    pub(super) dry_run: bool,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) berth_executable: &'a Path,
    pub(super) run_target: F,
}

/// Runs the prepared swarm targets and returns `(results, cancelled_count)`.
///
/// Cancellation behavior:
/// - Sequential mode stops immediately after the first failure when `fail_fast`.
/// - Parallel mode uses a shared atomic stop flag to avoid starting new work
///   after a failure signal.
pub(super) fn run_targets<F>(
    options: RunTargetsOptions<'_, F>,
) -> Result<(Vec<SwarmRunResult>, usize)>
where
    F: Fn(
            &Path,
            &ResolvedSwarmTarget,
            &[String],
            OutputMode,
            Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
        ) -> Result<SwarmRunResult>
        + Sync,
{
    crate::parallel::validate_parallelism(options.parallel)?;
    let build_args = |target: &ResolvedSwarmTarget| {
        super::args::swarm_child_args(
            target,
            options.command,
            options.port_strategy,
            options.port_seed,
            options.env_output,
            options.quiet,
            options.no_color,
            options.dry_run,
            options.runtime_env,
        )
    };

    if options.parallel <= 1 {
        return run_targets_sequential(
            options.targets,
            options.output_mode,
            options.fail_fast,
            options.berth_executable,
            build_args,
            options.run_target,
        );
    }

    parallel::run_targets_parallel(parallel::RunTargetsParallelOptions {
        targets: options.targets,
        output_mode: options.output_mode,
        parallel: options.parallel,
        fail_fast: options.fail_fast,
        berth_executable: options.berth_executable,
        build_args,
        run_target: options.run_target,
    })
}

fn run_targets_sequential<F, A>(
    targets: &[ResolvedSwarmTarget],
    output_mode: OutputMode,
    fail_fast: bool,
    berth_executable: &Path,
    build_args: A,
    run_target: F,
) -> Result<(Vec<SwarmRunResult>, usize)>
where
    F: Fn(
        &Path,
        &ResolvedSwarmTarget,
        &[String],
        OutputMode,
        Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<SwarmRunResult>,
    A: Fn(&ResolvedSwarmTarget) -> Vec<String>,
{
    let mut results = Vec::new();
    for target in targets {
        let args = build_args(target);
        let result = run_target(berth_executable, target, &args, output_mode, None)?;
        let failed = !result.success;
        results.push(result);
        if failed && fail_fast {
            break;
        }
    }
    Ok((results, 0))
}

#[cfg(test)]
mod tests {
    use super::{RunTargetsOptions, run_targets};
    use crate::swarm::target_exec::{OutputMode, SwarmRunResult};
    use crate::swarm::targets::ResolvedSwarmTarget;
    use std::path::PathBuf;

    #[test]
    fn run_targets_rejects_zero_parallelism() {
        let target = ResolvedSwarmTarget {
            name: "svc".to_owned(),
            root: PathBuf::from("/tmp/ignored"),
        };

        let result = run_targets(RunTargetsOptions {
            targets: std::slice::from_ref(&target),
            command: &["echo".to_owned()],
            output_mode: OutputMode::Logged,
            parallel: 0,
            fail_fast: false,
            port_strategy: crate::cli::args::PortStrategyArg::Stable,
            port_seed: None,
            env_output: false,
            quiet: false,
            no_color: false,
            dry_run: false,
            runtime_env: None,
            berth_executable: std::path::Path::new("/bin/echo"),
            run_target: run_target_result,
        });

        assert!(result.is_err());
    }

    #[test]
    fn run_targets_executes_targets_sequentially_by_default() {
        let targets = [
            ResolvedSwarmTarget {
                name: "svc-a".to_owned(),
                root: PathBuf::from("/tmp/ignored-a"),
            },
            ResolvedSwarmTarget {
                name: "svc-b".to_owned(),
                root: PathBuf::from("/tmp/ignored-b"),
            },
        ];
        let results = run_targets(RunTargetsOptions {
            targets: &targets,
            command: &["echo".to_owned()],
            output_mode: OutputMode::Logged,
            parallel: 1,
            fail_fast: false,
            port_strategy: crate::cli::args::PortStrategyArg::Stable,
            port_seed: None,
            env_output: false,
            quiet: false,
            no_color: false,
            dry_run: false,
            runtime_env: None,
            berth_executable: std::path::Path::new("/bin/echo"),
            run_target: run_target_logged,
        })
        .expect("run targets");

        let (results, cancelled) = results;
        assert_eq!(cancelled, 0);
        assert_eq!(results.len(), 2);
        assert!(results[0].success);
        assert!(results[1].success);
    }

    #[test]
    fn run_targets_respects_fail_fast_in_sequential_mode() {
        let targets = [
            ResolvedSwarmTarget {
                name: "svc-a".to_owned(),
                root: PathBuf::from("/tmp/ignored-a"),
            },
            ResolvedSwarmTarget {
                name: "svc-b".to_owned(),
                root: PathBuf::from("/tmp/ignored-b"),
            },
        ];
        let result = run_targets(RunTargetsOptions {
            targets: &targets,
            command: &["echo".to_owned()],
            output_mode: OutputMode::Logged,
            parallel: 1,
            fail_fast: true,
            port_strategy: crate::cli::args::PortStrategyArg::Stable,
            port_seed: None,
            env_output: false,
            quiet: false,
            no_color: false,
            dry_run: false,
            runtime_env: None,
            berth_executable: std::path::Path::new("/bin/echo"),
            run_target: fail_fast_target,
        })
        .expect("run targets with fail fast");

        let (results, cancelled) = result;
        assert_eq!(cancelled, 0);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn run_targets_uses_parallel_execution_when_enabled() {
        let targets = [
            ResolvedSwarmTarget {
                name: "svc-a".to_owned(),
                root: PathBuf::from("/tmp/ignored-a"),
            },
            ResolvedSwarmTarget {
                name: "svc-b".to_owned(),
                root: PathBuf::from("/tmp/ignored-b"),
            },
        ];
        let (results, cancelled) = run_targets(RunTargetsOptions {
            targets: &targets,
            command: &["echo".to_owned()],
            output_mode: OutputMode::Passthrough,
            parallel: 2,
            fail_fast: false,
            port_strategy: crate::cli::args::PortStrategyArg::Stable,
            port_seed: None,
            env_output: false,
            quiet: false,
            no_color: false,
            dry_run: false,
            runtime_env: None,
            berth_executable: std::path::Path::new("/bin/echo"),
            run_target: run_target_logged,
        })
        .expect("run targets");
        assert_eq!(cancelled, 0);
        assert_eq!(results.len(), 2);
    }

    fn run_target_result(
        _berth: &std::path::Path,
        _target: &ResolvedSwarmTarget,
        _args: &[String],
        _mode: OutputMode,
        _cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<SwarmRunResult, anyhow::Error> {
        Ok(SwarmRunResult {
            name: "svc".to_owned(),
            root: PathBuf::from("/tmp/ignored"),
            success: true,
            skipped: false,
        })
    }

    fn run_target_logged(
        _berth: &std::path::Path,
        target: &ResolvedSwarmTarget,
        _args: &[String],
        _mode: OutputMode,
        _cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<SwarmRunResult, anyhow::Error> {
        Ok(SwarmRunResult {
            name: target.name.clone(),
            root: target.root.clone(),
            success: true,
            skipped: false,
        })
    }

    fn fail_fast_target(
        _berth: &std::path::Path,
        target: &ResolvedSwarmTarget,
        _args: &[String],
        _mode: OutputMode,
        _cancelled: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    ) -> Result<SwarmRunResult, anyhow::Error> {
        Ok(SwarmRunResult {
            name: target.name.clone(),
            root: target.root.clone(),
            success: target.name != "svc-a",
            skipped: false,
        })
    }
}
