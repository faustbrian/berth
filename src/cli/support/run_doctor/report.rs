//! Shared doctor output helpers.
//!
//! Every check is logged on stderr and, while `doctor --json` records them,
//! kept for its document.

use std::sync::Mutex;

use crate::output::{self, LogLevel, Persistence};

/// One check result for `doctor --json`.
#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct DoctorCheck {
    /// `ok`, `info` or `error`.
    pub(crate) status: &'static str,
    pub(crate) message: String,
}

static RECORDED: Mutex<Option<Vec<DoctorCheck>>> = Mutex::new(None);

/// Starts keeping every check result.
pub(crate) fn start_recording() {
    *RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Vec::new());
}

/// Stops keeping check results and returns those kept.
pub(crate) fn take_recording() -> Vec<DoctorCheck> {
    RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take()
        .unwrap_or_default()
}

fn record(status: &'static str, message: &str) {
    if let Some(checks) = RECORDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_mut()
    {
        checks.push(DoctorCheck {
            status,
            message: message.to_owned(),
        });
    }
}

pub(super) fn success(message: &str) {
    record("ok", message);
    output::event(
        "doctor",
        LogLevel::Success,
        message,
        Persistence::Persistent,
    );
}

pub(super) fn info(message: &str) {
    record("info", message);
    output::event("doctor", LogLevel::Info, message, Persistence::Persistent);
}

pub(super) fn error(message: &str) {
    record("error", message);
    output::event("doctor", LogLevel::Error, message, Persistence::Persistent);
}
