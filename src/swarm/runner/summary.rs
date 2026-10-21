//! Result summarization for swarm runs.

use anyhow::Result;

use crate::output::LogLevel;

use super::super::target_exec::{OutputMode, SwarmRunResult};

/// Emits run summary output and converts failed targets into a terminal error.
///
/// Any non-skipped failure causes this function to return an error after
/// printing/logging per-target failure entries.
pub(super) fn summarize_results(
    targets: &[crate::swarm::targets::ResolvedSwarmTarget],
    results: &[SwarmRunResult],
    cancelled: usize,
    output_mode: OutputMode,
    rerun: &Rerun<'_>,
) -> Result<()> {
    let failures: Vec<&SwarmRunResult> = results
        .iter()
        .filter(|result| !result.success && !result.skipped)
        .collect();
    let succeeded = results
        .iter()
        .filter(|result| result.success && !result.skipped)
        .count();
    // Every target that neither succeeded nor failed: skipped, stopped by
    // `--fail-fast`, or never started (the sequential runner reports no
    // `cancelled` for those).
    let skipped = targets
        .len()
        .saturating_sub(succeeded + failures.len())
        .max(results.iter().filter(|result| result.skipped).count() + cancelled);
    emit_summary(output_mode, succeeded, failures.len(), skipped);

    if !failures.is_empty() {
        // The swarm exits with the first failing target's own exit code, so a
        // failing test in one project is never reported as a berth failure.
        let code = failures
            .iter()
            .find_map(|failure| failure.exit_code.filter(|code| *code != 0))
            .unwrap_or(1);
        let names = failures
            .iter()
            .map(|failure| failure.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        for failure in failures.iter().copied() {
            emit_failure(output_mode, failure);
        }
        // `--fail-fast` stops the rest: a target it killed is a skipped
        // result, one it never started has none. Both still have to run.
        let unfinished: Vec<&str> = targets
            .iter()
            .map(|target| target.name.as_str())
            .filter(|name| {
                results
                    .iter()
                    .find(|result| result.name == *name)
                    .is_none_or(|result| result.skipped)
            })
            .collect();
        let failed: Vec<&str> = failures
            .iter()
            .map(|failure| failure.name.as_str())
            .collect();
        let rerun_names: Vec<&str> = targets
            .iter()
            .map(|target| target.name.as_str())
            .filter(|name| failed.contains(name) || unfinished.contains(name))
            .collect();
        let (also, which) = if unfinished.is_empty() {
            (String::new(), "just those")
        } else {
            (
                format!(
                    " ({} stopped or never started after the failure)",
                    unfinished.join(", ")
                ),
                "the unfinished targets",
            )
        };
        let message = if rerun.dependency_run {
            let way = if rerun.command.first().is_some_and(|sub| sub == "down") {
                "down"
            } else {
                "up"
            };
            format!(
                "workspace dependencies failed: {names}{also}; fix them and run the same command \
                 again, which brings its dependencies {way} first"
            )
        } else {
            format!(
                "swarm command failed for: {names}{also}; rerun {which} with `{}`",
                rerun.command(&rerun_names.join(","))
            )
        };
        return Err(anyhow::Error::new(
            crate::docker::ChildExitError::from_code(code, message),
        ));
    }

    Ok(())
}

fn emit_summary(output_mode: OutputMode, succeeded: usize, failed: usize, skipped: usize) {
    let level = if failed == 0 {
        LogLevel::Success
    } else {
        LogLevel::Error
    };
    let message = match output_mode {
        OutputMode::Logged => {
            format!("Summary: {succeeded} succeeded, {failed} failed, {skipped} skipped")
        }
        OutputMode::Passthrough => {
            format!("Swarm summary: {succeeded} succeeded, {failed} failed, {skipped} skipped")
        }
    };
    super::output::emit_swarm(output_mode, level, &message);
}

fn emit_failure(output_mode: OutputMode, failure: &SwarmRunResult) {
    let message = format!(
        "Failed target '{}' at {}",
        failure.name,
        failure.root.display()
    );
    super::output::emit_swarm(output_mode, LogLevel::Error, &message);
}

/// What a rerun of the failed targets needs to do what the run did: the
/// workspace (the run may start from a member project), its runtime
/// environment, every swarm flag that changes the outcome, and the command.
pub(super) struct Rerun<'a> {
    pub(super) workspace_root: &'a std::path::Path,
    pub(super) runtime_env: Option<&'a str>,
    pub(super) include_deps: bool,
    pub(super) force_down_deps: bool,
    pub(super) port_strategy: crate::cli::args::PortStrategyArg,
    pub(super) port_seed: Option<&'a str>,
    /// `--parallel`: its default depends on the machine, and a run at 1
    /// keeps targets in order.
    pub(super) parallel: usize,
    pub(super) fail_fast: bool,
    pub(super) command: &'a [String],
    /// A member's dependency run: the member's own command reruns them
    /// (and then the member), without the workspace-level checks a `berth
    /// swarm` command adds (its `--repro` lockfile).
    pub(super) dependency_run: bool,
}

impl Rerun<'_> {
    fn command(&self, only: &str) -> String {
        use crate::followup::quote;
        let mut words = vec!["berth".to_owned()];
        words.extend(crate::followup::context_flags(self.workspace_root));
        if let Some(env) = self.runtime_env {
            words.extend(["--env".to_owned(), quote(env)]);
        }
        words.push("swarm".to_owned());
        // `--only` turns on the guard against stopping dependencies other
        // targets share, which a whole-workspace `down` never meets; its
        // dependencies were stopped already, so the rerun leaves them be.
        let down = self.command.first().is_some_and(|sub| sub == "down");
        if !self.include_deps || (down && !self.force_down_deps) {
            words.push("--no-deps".to_owned());
        }
        if self.force_down_deps {
            words.push("--force".to_owned());
        }
        if self.port_strategy == crate::cli::args::PortStrategyArg::Stable {
            words.extend(["--port-strategy".to_owned(), "stable".to_owned()]);
        }
        if let Some(seed) = self.port_seed {
            words.extend(["--port-seed".to_owned(), quote(seed)]);
        }
        words.extend(["--parallel".to_owned(), self.parallel.to_string()]);
        if self.fail_fast {
            words.push("--fail-fast".to_owned());
        }
        words.extend(["--only".to_owned(), quote(only)]);
        words.extend(self.command.iter().map(|word| quote(word)));
        words.join(" ")
    }
}

#[cfg(test)]
mod tests {
    fn targets(results: &[SwarmRunResult]) -> Vec<crate::swarm::targets::ResolvedSwarmTarget> {
        results
            .iter()
            .map(|result| crate::swarm::targets::ResolvedSwarmTarget {
                name: result.name.clone(),
                root: result.root.clone(),
            })
            .collect()
    }

    fn rerun() -> super::Rerun<'static> {
        super::Rerun {
            workspace_root: std::path::Path::new("/my ws"),
            runtime_env: Some("staging"),
            include_deps: false,
            force_down_deps: false,
            port_strategy: crate::cli::args::PortStrategyArg::Stable,
            port_seed: Some("s1"),
            parallel: 1,
            fail_fast: true,
            command: &[],
            dependency_run: false,
        }
    }

    use super::summarize_results;
    use crate::swarm::target_exec::{OutputMode, SwarmRunResult};
    use std::path::PathBuf;

    fn result(name: &str, success: bool, skipped: bool) -> SwarmRunResult {
        SwarmRunResult {
            name: name.to_owned(),
            root: PathBuf::from(format!("/tmp/{name}")),
            success,
            skipped,
            exit_code: if success { Some(0) } else { Some(3) },
        }
    }

    #[test]
    fn a_fail_fast_rerun_includes_the_targets_it_stopped() {
        let results = vec![
            result("alpha", true, false),
            result("beta", false, false),
            result("gamma", false, true),
        ];
        let mut all = targets(&results);
        all.push(crate::swarm::targets::ResolvedSwarmTarget {
            name: "delta".to_owned(),
            root: PathBuf::from("/tmp/delta"),
        });
        let error = summarize_results(&all, &results, 1, OutputMode::Logged, &rerun())
            .expect_err("failure")
            .to_string();
        assert!(
            error.contains("gamma, delta stopped or never started after the failure"),
            "{error}"
        );
        assert!(error.contains("--only beta,gamma,delta"), "{error}");
    }

    #[test]
    fn a_dependency_run_failure_reruns_the_members_own_command() {
        let results = vec![result("db", false, false)];
        let rerun = super::Rerun {
            dependency_run: true,
            ..rerun()
        };
        let error = summarize_results(&targets(&results), &results, 0, OutputMode::Logged, &rerun)
            .expect_err("failure")
            .to_string();
        assert!(
            error.contains("workspace dependencies failed: db"),
            "{error}"
        );
        assert!(error.contains("run the same command again"), "{error}");
        // A `berth swarm` rerun would add the workspace lockfile check.
        assert!(!error.contains("swarm --only"), "{error}");
    }

    #[test]
    fn a_down_rerun_does_not_trip_the_shared_dependency_guard() {
        let command = ["down".to_owned()];
        let rerun = super::Rerun {
            workspace_root: std::path::Path::new("/ws"),
            runtime_env: None,
            include_deps: true,
            force_down_deps: false,
            port_strategy: crate::cli::args::PortStrategyArg::Random,
            port_seed: None,
            parallel: 2,
            fail_fast: false,
            command: &command,
            dependency_run: false,
        };
        assert_eq!(
            rerun.command("api"),
            "berth --project-root /ws swarm --no-deps --parallel 2 --only api down"
        );
        let forced = super::Rerun {
            force_down_deps: true,
            ..rerun
        };
        // clap rejects `--force` with `--no-deps`.
        assert_eq!(
            forced.command("api"),
            "berth --project-root /ws swarm --force --parallel 2 --only api down"
        );
    }

    #[test]
    fn summarize_results_returns_ok_for_successful_targets() {
        let results = vec![result("alpha", true, false), result("beta", true, false)];
        assert!(
            summarize_results(
                &targets(&results),
                &results,
                0,
                OutputMode::Logged,
                &rerun()
            )
            .is_ok()
        );
    }

    #[test]
    fn summarize_results_ignores_skipped_in_failure_count() {
        let results = vec![
            result("alpha", false, true),
            result("beta", true, false),
            result("gamma", false, false),
        ];
        assert!(
            summarize_results(
                &targets(&results),
                &results,
                1,
                OutputMode::Passthrough,
                &rerun()
            )
            .is_err()
        );
    }

    #[test]
    fn summarize_results_fails_when_any_target_fails() {
        let results = vec![result("alpha", false, false)];
        assert!(
            summarize_results(
                &targets(&results),
                &results,
                0,
                OutputMode::Logged,
                &rerun()
            )
            .is_err()
        );
    }
    #[test]
    fn a_failing_target_passes_its_exit_code_through() {
        let results = vec![result("alpha", true, false), result("beta", false, false)];
        let error = summarize_results(
            &targets(&results),
            &results,
            0,
            OutputMode::Logged,
            &rerun(),
        )
        .expect_err("failure");
        let code = error
            .downcast_ref::<crate::docker::ChildExitError>()
            .map(crate::docker::ChildExitError::code);
        assert_eq!(code, Some(3));
        assert!(error.to_string().contains("beta"), "{error}");
        assert!(
            error.to_string().contains(
                "berth --project-root '/my ws' --env staging swarm --no-deps \
                     --port-strategy stable --port-seed s1 --parallel 1 --fail-fast --only beta"
            ),
            "{error}"
        );
    }
}
