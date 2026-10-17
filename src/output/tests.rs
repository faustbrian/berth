//! output tests module.
//!
//! Contains output tests logic used by Berth command workflows.

use super::{
    LogLevel, Persistence,
    colorize::{TokenColor, colorize_message_payload, colorize_message_tokens, token_color},
    entry::LogEntry,
    normalize::{
        is_fractional_unix_timestamp, normalize_log_message, strip_ansi_codes,
        strip_leading_bracket_timestamp, strip_leading_laravel_prefix,
    },
};
use serde_json::json;
use time::OffsetDateTime;

#[test]
fn strip_leading_epoch_timestamp_for_stream_lines() {
    let line = "[1770869968.369] [db] [ok] Purged container 'acme-bill-db'";
    assert_eq!(
        strip_leading_bracket_timestamp(line),
        "[db] [ok] Purged container 'acme-bill-db'"
    );
}

#[test]
fn keep_message_without_epoch_prefix_unchanged() {
    let line = "[2025-12-25 05:32:31] local.INFO: Migrating";
    assert_eq!(strip_leading_bracket_timestamp(line), line);
}

#[test]
fn strip_leading_laravel_prefix_extracts_level_and_body() {
    let line = "[2026-02-12 06:31:09] local.SUCCESS: [caddy] Reloaded config";
    let (level, body): (Option<LogLevel>, &str) = strip_leading_laravel_prefix(line);
    assert!(matches!(level, Some(LogLevel::Success)));
    assert_eq!(body, "[caddy] Reloaded config");
}

#[test]
fn strip_leading_laravel_prefix_accepts_dot_padded_level() {
    let line = "[2026-02-12 06:31:09] local.SUCCESS..: [mailhog] Started";
    let (level, body) = strip_leading_laravel_prefix(line);
    assert!(matches!(level, Some(LogLevel::Success)));
    assert_eq!(body, "[mailhog] Started");
}

#[test]
fn strip_ansi_codes_removes_escape_sequences() {
    let line = "\u{1b}[32m[2026-02-12 06:31:09] local.SUCCESS..: ok\u{1b}[0m";
    assert_eq!(
        strip_ansi_codes(line),
        "[2026-02-12 06:31:09] local.SUCCESS..: ok"
    );
}

/// Normalizes log message enforces consistent style into a canonical form.
#[test]
fn normalize_log_message_enforces_consistent_style() {
    assert_eq!(
        normalize_log_message(
            LogLevel::Info,
            "   recreating   service \ton random port  12345.  "
        ),
        "Recreating service on random port 12345"
    );
}

/// Normalizes log message handles empty message into a canonical form.
#[test]
fn normalize_log_message_handles_empty_message() {
    assert_eq!(normalize_log_message(LogLevel::Info, "   \n\t "), "(empty)");
}

/// Normalizes log message keeps bracket tags lowercase into a canonical form.
#[test]
fn normalize_log_message_keeps_bracket_tags_lowercase() {
    assert_eq!(
        normalize_log_message(LogLevel::Info, "[app] started container for serve target"),
        "[app] Started container for serve target"
    );
}

/// Normalizes log message rewrites skipping to skipped into a canonical form.
#[test]
fn normalize_log_message_rewrites_skipping_to_skipped() {
    assert_eq!(
        normalize_log_message(
            LogLevel::Info,
            "Skipping container CA trust because container_port is 80",
        ),
        "Skipped container CA trust because container_port is 80"
    );
}

/// Normalizes log message rewrites error prefixes into a canonical form.
#[test]
fn normalize_log_message_rewrites_error_prefixes() {
    assert_eq!(
        normalize_log_message(
            LogLevel::Error,
            "Failed to run serve container: port is already allocated",
        ),
        "Run serve container failed: port is already allocated"
    );
    assert_eq!(
        normalize_log_message(LogLevel::Error, "Error: docker daemon is not reachable"),
        "Operation failed: docker daemon is not reachable"
    );
}

#[test]
fn colorize_message_payload_highlights_semantic_values() {
    let raw = "Recreating service on random port 58835 using 'berth/acme-api-app' at https://acme-api.grid with config /Users/brian/.config/berth/caddy/Caddyfile";
    let rendered = colorize_message_payload(raw);
    assert_eq!(strip_ansi_codes(&rendered), raw);
}

/// Normalizes log message strips simple identifier quotes into a canonical form.
#[test]
fn normalize_log_message_strips_simple_identifier_quotes() {
    assert_eq!(
        normalize_log_message(
            LogLevel::Info,
            "Using cached derived image 'berth/acme-api-app-serve-e5b602cccc6e69c0'",
        ),
        "Using cached derived image berth/acme-api-app-serve-e5b602cccc6e69c0"
    );
    assert_eq!(
        normalize_log_message(LogLevel::Success, "Purged container 'acme-api-s3'"),
        "Purged container acme-api-s3"
    );
}

#[test]
fn colorize_message_tokens_normalizes_nested_spacing() {
    let db = strip_ansi_codes(&colorize_message_tokens("[bill] [db] Recreating service"));
    let valkey = strip_ansi_codes(&colorize_message_tokens(
        "[bill] [valkey] Recreating service",
    ));

    let db_start = db.find("Recreating service").expect("db start");
    let valkey_start = valkey.find("Recreating service").expect("valkey start");
    assert_eq!(db_start, valkey_start);
}

#[test]
fn colorize_message_tokens_aligns_single_token_lines_with_three_token_lines() {
    let swarm = strip_ansi_codes(&colorize_message_tokens(
        "[swarm] Running `berth recreate` across 6 target(s)",
    ));
    let bill = strip_ansi_codes(&colorize_message_tokens("[bill] [db] Recreating service"));

    let swarm_start = swarm.find("Running berth recreate").expect("swarm start");
    let bill_start = bill.find("Recreating service").expect("bill start");
    assert_eq!(swarm_start, bill_start);
}

#[test]
fn colorize_message_tokens_keeps_separator_for_adjacent_tokens() {
    let rendered = strip_ansi_codes(&colorize_message_tokens("[postal][db] Recreating service"));
    let postal_idx = rendered.find("[postal]").expect("postal token");
    let db_idx = rendered.find("[db]").expect("db token");
    assert!(db_idx > postal_idx + "[postal]".len());
}

#[test]
fn colorize_message_tokens_aligns_first_column_for_bill_and_postal() {
    let bill = strip_ansi_codes(&colorize_message_tokens("[bill] [db] Recreating service"));
    let postal = strip_ansi_codes(&colorize_message_tokens("[postal] [db] Recreating service"));

    let bill_start = bill.find("Recreating service").expect("bill start");
    let postal_start = postal.find("Recreating service").expect("postal start");
    assert_eq!(bill_start, postal_start);
}

#[test]
fn strip_leading_laravel_prefix_ignores_non_laravel_messages() {
    let line = "[api] plain message";
    let (level, body) = strip_leading_laravel_prefix(line);
    assert!(level.is_none());
    assert_eq!(body, line);
}

#[test]
fn recognizes_fractional_unix_timestamps_only() {
    assert!(is_fractional_unix_timestamp("1770869968.369"));
    assert!(!is_fractional_unix_timestamp("2025-12-25 05:32:31"));
    assert!(!is_fractional_unix_timestamp("abc.123"));
}

/// Renders line uses laravel style and context suffix for command execution.
#[test]
fn render_line_uses_laravel_style_and_context_suffix() {
    let entry = LogEntry {
        timestamp: OffsetDateTime::UNIX_EPOCH,
        level: LogLevel::Info,
        message: String::from("[db] Migrating files"),
        context: Some(json!({"log_channel": "stack"})),
        persistence: Persistence::Persistent,
    };

    let rendered = entry.render_file_line();

    assert_eq!(
        rendered,
        "[1970-01-01 00:00:00] local.INFO.....: [db] Migrating files  {\"log_channel\":\"stack\"}"
    );
}

#[test]
fn quiet_visibility_matches_warning_and_above() {
    assert!(LogLevel::Warn.visible_when_quiet());
    assert!(LogLevel::Error.visible_when_quiet());
    assert!(!LogLevel::Info.visible_when_quiet());
    assert!(!LogLevel::Debug.visible_when_quiet());
}

#[test]
fn token_color_maps_brand_services() {
    assert!(matches!(token_color("app"), TokenColor::Orange));
    assert!(matches!(token_color("laravel"), TokenColor::Orange));
    assert!(matches!(token_color("frankenphp"), TokenColor::Orange));
    assert!(matches!(token_color("redis"), TokenColor::RedisRed));
    assert!(matches!(token_color("valkey"), TokenColor::RedisRed));
    assert!(matches!(token_color("memcached"), TokenColor::RedisRed));
    assert!(matches!(token_color("postgres"), TokenColor::PostgresBlue));
    assert!(matches!(token_color("pg"), TokenColor::PostgresBlue));
    assert!(matches!(token_color("mongodb"), TokenColor::Green));
    assert!(matches!(token_color("mysql"), TokenColor::MysqlBlue));
    assert!(matches!(token_color("mariadb"), TokenColor::MariaBlue));
    assert!(matches!(token_color("minio"), TokenColor::MinioRed));
    assert!(matches!(token_color("garage"), TokenColor::MinioRed));
    assert!(matches!(token_color("rustfs"), TokenColor::RustOrange));
    assert!(matches!(
        token_color("meilisearch"),
        TokenColor::SearchGreen
    ));
    assert!(matches!(token_color("opensearch"), TokenColor::Teal));
    assert!(matches!(token_color("elasticsearch"), TokenColor::Teal));
    assert!(matches!(token_color("typesense"), TokenColor::Teal));
    assert!(matches!(token_color("gotenberg"), TokenColor::BrandBlue));
    assert!(matches!(token_color("rabbitmq"), TokenColor::Purple));
    assert!(matches!(token_color("soketi"), TokenColor::Purple));
    assert!(matches!(token_color("scheduler"), TokenColor::Purple));
    assert!(matches!(token_color("selenium"), TokenColor::Purple));
    assert!(matches!(token_color("mailpit"), TokenColor::Purple));
}
