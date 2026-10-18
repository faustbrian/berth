//! Telling the engine's failure to run a command apart from the command's.

/// Whether the engine, not the command, failed: a restarting, stopped or
/// overloaded engine (Docker's "Error response from daemon", an OCI runtime
/// error, a timeout, Podman's exec-session refusal). Asking again may work,
/// so a readiness wait retries instead of blaming the service.
#[must_use]
pub(crate) fn is_engine_exec_failure(output: &str) -> bool {
    [
        "Error response from daemon",
        "OCI runtime exec failed",
        "resource temporarily unavailable",
        "context deadline exceeded",
        "Cannot connect to the Docker daemon",
        "error during connect",
        "can only create exec sessions on running containers",
    ]
    .iter()
    .any(|error| output.contains(error))
}

#[cfg(test)]
mod tests {
    use super::is_engine_exec_failure;

    #[test]
    fn engine_failures_are_told_from_the_commands_own() {
        for engine in [
            "OCI runtime exec failed: exec failed: read init-p: connection reset by peer: unknown",
            "Error response from daemon: Container x is restarting",
            "Error: can only create exec sessions on running containers: container state improper",
        ] {
            assert!(is_engine_exec_failure(engine), "{engine}");
        }
        assert!(!is_engine_exec_failure(
            "PHP Fatal error:  Uncaught Error in /app/bootstrap/app.php:9"
        ));
    }
}
