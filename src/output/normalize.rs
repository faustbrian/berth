//! output normalize module.
//!
//! Contains output normalization helpers used by Berth command workflows.

use super::LogLevel;
mod ansi;
mod rewrite;
mod timestamps;
pub(super) use ansi::strip_ansi_codes;
use rewrite::rewrite_message_for_level;
pub(super) use timestamps::is_fractional_unix_timestamp;
use timestamps::is_laravel_timestamp;

pub(super) fn strip_leading_bracket_timestamp(message: &str) -> &str {
    if let Some(rest) = message.strip_prefix('[')
        && let Some((inside, tail)) = rest.split_once(']')
        && is_fractional_unix_timestamp(inside)
    {
        return tail.trim_start();
    }

    message
}

pub(super) fn strip_leading_laravel_prefix(message: &str) -> (Option<LogLevel>, &str) {
    let Some(rest) = message.strip_prefix('[') else {
        return (None, message);
    };
    let Some((timestamp, suffix)) = rest.split_once("] ") else {
        return (None, message);
    };
    if !is_laravel_timestamp(timestamp) {
        return (None, message);
    }

    let Some(after_local) = suffix.strip_prefix("local.") else {
        return (None, message);
    };
    let Some((level_padded, body)) = after_local.split_once(": ") else {
        return (None, message);
    };
    let level_raw = level_padded.trim_end_matches('.');

    (LogLevel::from_laravel(level_raw), body)
}

/// Normalizes log message into a canonical form.
pub(super) fn normalize_log_message(level: LogLevel, message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return String::from("(empty)");
    }

    let mut normalized = String::with_capacity(trimmed.len());
    let mut in_space = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !in_space {
                normalized.push(' ');
                in_space = true;
            }
            continue;
        }
        normalized.push(ch);
        in_space = false;
    }

    while normalized.ends_with('.') {
        normalized.pop();
    }

    rewrite_message_for_level(level, normalized)
}
