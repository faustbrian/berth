//! output module.
//!
//! Contains output logic used by Berth command workflows.

use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;

use emit::{emit_entry, emit_entry_direct, now_local_timestamp};
use init::{install_logger, tracing_fallback_warning};
use normalize::normalize_log_message;
use serde_json::Value;
use state::LoggerState;
use stream::prepare_stream_message;
use terminal_line::terminal_line_for_stderr;
pub(crate) use types::{Channel, LogLevel, Persistence};

static LOGGER_STATE: OnceLock<LoggerState> = OnceLock::new();
static TRACING_ACTIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn init(quiet: bool) {
    if !install_logger(quiet) {
        let warning = tracing_fallback_warning(now_local_timestamp());
        emit_entry_direct(&warning);
    }
}

pub(crate) fn event(scope: &str, level: LogLevel, message: &str, persistence: Persistence) {
    event_with_context(scope, level, message, None, persistence);
}

pub(crate) fn event_with_context(
    scope: &str,
    level: LogLevel,
    message: &str,
    context: Option<Value>,
    persistence: Persistence,
) {
    let normalized = normalize_log_message(level, message);
    emit_entry(
        level,
        prefixed_scope_message(scope, &normalized),
        context,
        persistence,
    );
}

pub(crate) fn stream(scope: &str, channel: Channel, message: &str, persistence: Persistence) {
    stream_with_context(scope, channel, message, None, persistence);
}

pub(crate) fn stream_with_context(
    scope: &str,
    _channel: Channel,
    message: &str,
    context: Option<Value>,
    persistence: Persistence,
) {
    let (level, standardized) = prepare_stream_message(message);
    let message = format!("[{scope}] {standardized}");
    emit_entry(level, message, context, persistence);
}

fn prefixed_scope_message(scope: &str, message: &str) -> String {
    format!("[{scope}] {message}")
}

mod colorize;
mod emit;
mod entry;
mod init;
mod layer;
mod normalize;
mod state;
mod stream;
mod terminal_line;
#[cfg(test)]
mod tests;
mod tracing_emit;
mod types;
