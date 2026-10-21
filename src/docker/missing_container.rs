//! Whether an engine's error says a container does not exist.

/// Whether `stderr` says the container is gone: Docker's "No such
/// container" or (from `inspect`) "No such object", Podman's "no such
/// container". Not a bare "no such": an unreachable engine says "no such
/// file or directory" (its socket) or "no such host", which says nothing
/// about the container.
pub(crate) fn reports_missing_container(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("no such container") || stderr.contains("no such object")
}
