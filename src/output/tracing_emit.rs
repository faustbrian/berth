//! output tracing emit module.
//!
//! Contains tracing event emission helpers used by Berth command workflows.

use tracing::Level;

pub(super) fn emit_tracing_event(
    level: Level,
    laravel_level: &str,
    message: &str,
    context: Option<&str>,
    persistence: &str,
) {
    match (level, context) {
        (Level::ERROR, Some(json)) => tracing::event!(
            Level::ERROR,
            log_level = laravel_level,
            body = %message,
            context = %json,
            persistence = persistence,
            "berth_log"
        ),
        (Level::WARN, Some(json)) => tracing::event!(
            Level::WARN,
            log_level = laravel_level,
            body = %message,
            context = %json,
            persistence = persistence,
            "berth_log"
        ),
        (Level::INFO, Some(json)) => tracing::event!(
            Level::INFO,
            log_level = laravel_level,
            body = %message,
            context = %json,
            persistence = persistence,
            "berth_log"
        ),
        (Level::DEBUG, Some(json)) => tracing::event!(
            Level::DEBUG,
            log_level = laravel_level,
            body = %message,
            context = %json,
            persistence = persistence,
            "berth_log"
        ),
        (Level::TRACE, Some(json)) => tracing::event!(
            Level::TRACE,
            log_level = laravel_level,
            body = %message,
            context = %json,
            persistence = persistence,
            "berth_log"
        ),
        (Level::ERROR, None) => tracing::event!(
            Level::ERROR,
            log_level = laravel_level,
            body = %message,
            persistence = persistence,
            "berth_log"
        ),
        (Level::WARN, None) => tracing::event!(
            Level::WARN,
            log_level = laravel_level,
            body = %message,
            persistence = persistence,
            "berth_log"
        ),
        (Level::INFO, None) => tracing::event!(
            Level::INFO,
            log_level = laravel_level,
            body = %message,
            persistence = persistence,
            "berth_log"
        ),
        (Level::DEBUG, None) => tracing::event!(
            Level::DEBUG,
            log_level = laravel_level,
            body = %message,
            persistence = persistence,
            "berth_log"
        ),
        (Level::TRACE, None) => tracing::event!(
            Level::TRACE,
            log_level = laravel_level,
            body = %message,
            persistence = persistence,
            "berth_log"
        ),
    };
}

#[cfg(test)]
mod tests {
    use tracing::Level;

    use super::emit_tracing_event;

    #[test]
    fn emit_tracing_event_handles_no_context() {
        emit_tracing_event(Level::INFO, "INFO", "startup complete", None, "persistent");
        emit_tracing_event(Level::DEBUG, "DEBUG", "cache refreshed", None, "transient");
        emit_tracing_event(Level::WARN, "WARN", "retry requested", None, "persistent");
    }

    #[test]
    fn emit_tracing_event_handles_context() {
        emit_tracing_event(
            Level::ERROR,
            "ERROR",
            "service failed",
            Some("{\"service\":\"api\"}"),
            "persistent",
        );
        emit_tracing_event(
            Level::TRACE,
            "TRACE",
            "debug trace available",
            Some("{\"scope\":\"health\"}"),
            "transient",
        );
    }
}
