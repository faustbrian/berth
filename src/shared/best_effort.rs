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
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BEST_EFFORT.store(false, Ordering::SeqCst);
        }
    }
    BEST_EFFORT.store(true, Ordering::SeqCst);
    let _reset = Reset;
    work()
}

/// Whether shared-server waits are skipped (see [`best_effort`]).
pub(super) fn is_best_effort() -> bool {
    BEST_EFFORT.load(Ordering::SeqCst)
}
