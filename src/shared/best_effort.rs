//! Best-effort shared-server work: reaping on behalf of another command.
//!
//! Another instance's leftovers must never block a command (8.x FI-53), so
//! while reaping in passing a shared server whose lock another process holds,
//! or that does not answer yet, is skipped instead of waited for; the next
//! pass or `berth gc` retries it.

use std::sync::atomic::{AtomicBool, Ordering};

static BEST_EFFORT: AtomicBool = AtomicBool::new(false);

/// Runs `work` with shared-server waits skipped instead of blocking.
pub(crate) fn best_effort<T>(work: impl FnOnce() -> T) -> T {
    /// Puts back the mode `work` began in: a nested call inside an outer
    /// one keeps it on.
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            BEST_EFFORT.store(self.0, Ordering::SeqCst);
        }
    }
    let _reset = Reset(BEST_EFFORT.swap(true, Ordering::SeqCst));
    work()
}

/// Whether shared-server waits are skipped (see [`best_effort`]).
pub(super) fn is_best_effort() -> bool {
    BEST_EFFORT.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::{best_effort, is_best_effort};

    #[test]
    fn a_nested_call_keeps_the_outer_mode() {
        best_effort(|| {
            best_effort(|| assert!(is_best_effort()));
            assert!(is_best_effort(), "the outer call is still best-effort");
        });
        assert!(!is_best_effort());
    }
}
