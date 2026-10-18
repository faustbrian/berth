//! `--tmpfs` options that mean the same on every engine.

use crate::config::ContainerEngine;

/// Returns `options` for a `--tmpfs` mount on the current engine.
///
/// Podman copies the image's directory at the mount point into the tmpfs
/// by default (`tmpcopyup`), Docker never does. A non-root container then
/// finds the image's root-owned subdirectories in its "empty" tmpfs (the
/// FrankenPHP image's `/data/caddy`, mode 0755), so Podman mounts get
/// `notmpcopyup`, which Docker would reject.
pub(crate) fn engine_tmpfs_options(options: &str) -> String {
    match super::container_engine() {
        ContainerEngine::Docker => options.to_owned(),
        ContainerEngine::Podman => format!("{options},notmpcopyup"),
    }
}

#[cfg(test)]
mod tests {
    use super::engine_tmpfs_options;
    use crate::config::ContainerEngine;

    #[test]
    fn only_podman_disables_copying_the_image_into_a_tmpfs() {
        assert_eq!(engine_tmpfs_options("rw,size=1m"), "rw,size=1m");
        crate::docker::with_container_engine(ContainerEngine::Podman, || {
            assert_eq!(engine_tmpfs_options("rw,size=1m"), "rw,size=1m,notmpcopyup");
        });
    }
}
