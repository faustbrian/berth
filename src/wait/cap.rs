//! The optional cap from `--wait-timeout` (or `setup --timeout`), for the
//! waits a command runs on its way to readiness besides the per-service
//! readiness waits (shared servers, the gateway, object stores). Each wait
//! gets the whole cap from the moment it starts, as a per-service wait does:
//! lock waits, pulls and builds before it never use it up.

use anyhow::Result;
use std::sync::Mutex;
use std::time::{Duration, Instant};

static CAP_SECS: Mutex<Option<u64>> = Mutex::new(None);

/// Sets the cap in seconds, or clears it.
pub(crate) fn set_cap(secs: Option<u64>) {
    *CAP_SECS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = secs;
}

/// The cap in seconds, if the command was given one.
#[must_use]
pub(crate) fn cap_secs() -> Option<u64> {
    *CAP_SECS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One wait's deadline: the cap counted from when the wait started.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Deadline {
    at: Option<(Instant, u64)>,
}

impl Deadline {
    /// Starts a wait under the command's cap.
    #[must_use]
    pub(crate) fn start() -> Self {
        Self::from_secs(cap_secs(), Instant::now())
    }

    fn from_secs(secs: Option<u64>, now: Instant) -> Self {
        Self {
            at: secs.map(|secs| (now + Duration::from_secs(secs), secs)),
        }
    }

    /// Fails once the wait's cap has passed, naming what it waited for.
    ///
    /// # Errors
    ///
    /// Returns an error after the deadline.
    pub(crate) fn ensure(self, waiting_for: &str) -> Result<()> {
        self.check(Instant::now(), waiting_for)
    }

    fn check(self, now: Instant, waiting_for: &str) -> Result<()> {
        if let Some((deadline, secs)) = self.at
            && now >= deadline
        {
            anyhow::bail!("{waiting_for} was not ready within the --wait-timeout of {secs}s");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_cap_never_fails_and_a_passed_cap_names_the_wait() {
        let now = Instant::now();
        Deadline::from_secs(None, now)
            .check(now + Duration::from_secs(3600), "the gateway")
            .expect("no cap, no deadline");
        let wait = Deadline::from_secs(Some(5), now);
        wait.check(now + Duration::from_secs(4), "the gateway")
            .expect("before the deadline");
        let error = wait
            .check(now + Duration::from_secs(5), "the gateway")
            .expect_err("cap passed");
        assert!(error.to_string().contains("the gateway"), "{error}");
        assert!(
            error.to_string().contains("--wait-timeout of 5s"),
            "{error}"
        );
    }

    #[test]
    fn each_wait_counts_the_cap_from_its_own_start() {
        let first = Instant::now();
        let later = first + Duration::from_secs(60);
        Deadline::from_secs(Some(30), later)
            .check(later + Duration::from_secs(29), "a shared server")
            .expect("a wait that starts after a long lock wait still has its whole cap");
    }
}
