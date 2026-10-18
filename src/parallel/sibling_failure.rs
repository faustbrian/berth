//! Ends the waits of parallel service runs once one of them fails.
//!
//! Waits have no deadline, so a service still starting would keep a
//! parallel `up` alive long after a sibling failed for real.

use anyhow::Result;
use rayon::prelude::*;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

thread_local! {
    /// The failure flag of the parallel run this thread works for, so a
    /// wait outside any parallel run (or in another one) is never ended.
    static RUN_FAILED: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// The error a wait ends with once a sibling run failed; the sibling's own
/// error is the one reported.
#[derive(Debug)]
pub(crate) struct SiblingFailed;

impl std::fmt::Display for SiblingFailed {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stopped waiting: another service failed")
    }
}

impl std::error::Error for SiblingFailed {}

/// Fails with [`SiblingFailed`] once a parallel sibling run failed. Every
/// wait loop calls it between attempts.
///
/// # Errors
///
/// Returns [`SiblingFailed`] after a sibling failed.
pub(crate) fn ensure_no_sibling_failed() -> Result<()> {
    let failed = RUN_FAILED.with(|flag| {
        flag.borrow()
            .as_ref()
            .is_some_and(|failed| failed.load(Ordering::SeqCst))
    });
    if failed {
        return Err(SiblingFailed.into());
    }
    Ok(())
}

/// Runs `run` for every item in parallel and returns the first real
/// failure, ending the other runs' waits as soon as it happens.
///
/// # Errors
///
/// Returns the first error a run failed with.
pub(crate) fn try_for_each_ending_siblings<T: Sync>(
    items: &[T],
    run: impl Fn(&T) -> Result<()> + Sync + Send,
) -> Result<()> {
    let failed = Arc::new(AtomicBool::new(false));
    let first = Mutex::new(None);
    let result = items.par_iter().try_for_each(|item| {
        let previous = RUN_FAILED.with(|flag| flag.replace(Some(Arc::clone(&failed))));
        let outcome = run(item);
        RUN_FAILED.with(|flag| flag.replace(previous));
        outcome.map_err(|error| {
            if error.downcast_ref::<SiblingFailed>().is_some() {
                return error;
            }
            failed.store(true, Ordering::SeqCst);
            let mut slot = first
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if slot.is_none() {
                *slot = Some(error);
            }
            SiblingFailed.into()
        })
    });
    match first
        .into_inner()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
    {
        Some(error) => Err(error),
        None => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn a_failing_run_ends_a_sibling_that_would_wait_forever() {
        let started = Instant::now();
        let error = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .expect("pool")
            .install(|| {
                try_for_each_ending_siblings(&["fails", "waits"], |item| {
                    if *item == "fails" {
                        std::thread::sleep(Duration::from_millis(100));
                        anyhow::bail!("db failed for real");
                    }
                    loop {
                        ensure_no_sibling_failed()?;
                        std::thread::sleep(Duration::from_millis(10));
                    }
                })
            })
            .expect_err("the run fails");
        assert_eq!(
            error.to_string(),
            "db failed for real",
            "the real error is reported"
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        ensure_no_sibling_failed().expect("a wait outside the run is never ended");
    }
}
