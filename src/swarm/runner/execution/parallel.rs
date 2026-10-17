//! Parallel swarm target execution helpers.

use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::swarm::target_exec::OutputMode;
use crate::swarm::target_exec::SwarmRunResult;
use crate::swarm::targets::ResolvedSwarmTarget;

pub(super) struct RunTargetsParallelOptions<'a, F, B> {
    pub(super) targets: &'a [ResolvedSwarmTarget],
    pub(super) output_mode: OutputMode,
    pub(super) parallel: usize,
    pub(super) fail_fast: bool,
    pub(super) berth_executable: &'a Path,
    pub(super) build_args: B,
    pub(super) run_target: F,
}

pub(super) fn run_targets_parallel<F, B>(
    options: RunTargetsParallelOptions<'_, F, B>,
) -> Result<(Vec<SwarmRunResult>, usize)>
where
    F: Fn(
            &Path,
            &ResolvedSwarmTarget,
            &[String],
            OutputMode,
            Option<Arc<AtomicBool>>,
        ) -> Result<SwarmRunResult>
        + Sync,
    B: Fn(&ResolvedSwarmTarget) -> Vec<String> + Sync,
{
    let stop = Arc::new(AtomicBool::new(false));
    let total = options.targets.len();
    let results = rayon::ThreadPoolBuilder::new()
        .num_threads(options.parallel)
        .build()?
        .install(|| {
            options
                .targets
                .par_iter()
                .filter_map(|target| {
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    let args = (options.build_args)(target);
                    let result = (options.run_target)(
                        options.berth_executable,
                        target,
                        &args,
                        options.output_mode,
                        Some(stop.clone()),
                    );
                    if options.fail_fast && result.as_ref().map(|run| !run.success).unwrap_or(true)
                    {
                        stop.store(true, Ordering::Relaxed);
                    }
                    Some(result)
                })
                .collect::<Result<Vec<_>>>()
        })?;

    let cancelled = total.saturating_sub(results.len());
    Ok((results, cancelled))
}
