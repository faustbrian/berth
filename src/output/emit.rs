//! Output emission helpers for tracing/direct logger paths.

use serde_json::Value;
use std::sync::atomic::Ordering;
use time::OffsetDateTime;

use super::entry::LogEntry;
use super::{LOGGER_STATE, LogLevel, Persistence, TRACING_ACTIVE, tracing_emit};

pub(super) fn emit_entry(
    level: LogLevel,
    message: String,
    context: Option<Value>,
    persistence: Persistence,
) {
    if !TRACING_ACTIVE.load(Ordering::Relaxed) {
        let entry = build_direct_entry(level, message, context, persistence);
        emit_entry_direct(&entry);
        return;
    }

    emit_via_tracing(level, &message, context.as_ref(), persistence);
}

pub(super) fn emit_entry_direct(entry: &LogEntry) {
    if let Some(logger) = LOGGER_STATE.get() {
        logger.emit_terminal(entry);
        logger.persist(entry);
        return;
    }

    eprintln!("{}", super::terminal_line_for_stderr(entry));
}

pub(super) fn now_local_timestamp() -> OffsetDateTime {
    match OffsetDateTime::now_local() {
        Ok(local) => local,
        Err(_) => OffsetDateTime::now_utc(),
    }
}

fn build_direct_entry(
    level: LogLevel,
    message: String,
    context: Option<Value>,
    persistence: Persistence,
) -> LogEntry {
    LogEntry {
        timestamp: now_local_timestamp(),
        level,
        message,
        context,
        persistence,
    }
}

fn emit_via_tracing(
    level: LogLevel,
    message: &str,
    context: Option<&Value>,
    persistence: Persistence,
) {
    let tracing_level = level.as_tracing_level();
    let context_string = context.map(ToString::to_string);
    tracing_emit::emit_tracing_event(
        tracing_level,
        level.as_laravel(),
        message,
        context_string.as_deref(),
        persistence.as_str(),
    );
}
