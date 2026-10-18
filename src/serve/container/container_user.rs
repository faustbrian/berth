//! Which user a PHP app container runs as, so that files it writes into the
//! bind-mounted checkout belong to the invoking user (ledger L023, FI-13).

use anyhow::{Context, Result};
use std::sync::OnceLock;

use crate::config::ContainerEngine;

/// The UID and GID berth acts for: the sudo caller's when berth runs under
/// `sudo` (root with every capability dropped could not write their
/// checkout), else the process's effective IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InvokingUser {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
}

/// Returns the invoking user.
///
/// # Errors
///
/// Returns an error when berth runs as root under sudo with a malformed
/// `SUDO_UID` or `SUDO_GID`.
pub(crate) fn invoking_user() -> Result<InvokingUser> {
    let uid = rustix::process::geteuid().as_raw();
    let gid = rustix::process::getegid().as_raw();
    if uid != 0 {
        return Ok(InvokingUser { uid, gid });
    }
    match (std::env::var("SUDO_UID"), std::env::var("SUDO_GID")) {
        (Ok(sudo_uid), Ok(sudo_gid)) => Ok(InvokingUser {
            uid: sudo_uid
                .parse()
                .with_context(|| format!("SUDO_UID '{sudo_uid}' is not a user ID"))?,
            gid: sudo_gid
                .parse()
                .with_context(|| format!("SUDO_GID '{sudo_gid}' is not a group ID"))?,
        }),
        _ => Ok(InvokingUser { uid, gid }),
    }
}

/// The `docker run` user options for the active engine:
/// - rootful Docker: `--user UID:GID`, the host IDs used as is;
/// - rootless Docker: none, because the container's root already is the
///   host user (a host UID inside would map to a subordinate ID that does
///   not own the checkout);
/// - Podman: `--userns=keep-id` (rootless by default) plus `--user UID:GID`,
///   which keep-id maps to the same host IDs.
///
/// # Errors
///
/// Returns an error when the invoking user or Docker's rootless mode cannot
/// be determined.
pub(crate) fn container_user_args() -> Result<Vec<String>> {
    let user = invoking_user()?;
    user_args_for(user.uid, user.gid)
}

/// [`container_user_args`] for explicit host IDs (the gateway runs as the
/// owner of its state directories).
///
/// # Errors
///
/// Returns an error when Docker's rootless mode cannot be determined.
pub(crate) fn user_args_for(uid: u32, gid: u32) -> Result<Vec<String>> {
    let ids = format!("{uid}:{gid}");
    match crate::docker::container_engine() {
        ContainerEngine::Podman => Ok(vec![
            "--userns=keep-id".to_owned(),
            "--user".to_owned(),
            ids,
        ]),
        ContainerEngine::Docker if docker_is_rootless()? => Ok(Vec::new()),
        ContainerEngine::Docker => Ok(vec!["--user".to_owned(), ids]),
    }
}

/// Whether the Docker daemon runs rootless, asked once per process.
fn docker_is_rootless() -> Result<bool> {
    // Unit tests never reach a real engine; they model rootful Docker.
    if cfg!(test) {
        return Ok(false);
    }
    static ROOTLESS: OnceLock<std::result::Result<bool, String>> = OnceLock::new();
    ROOTLESS
        .get_or_init(|| {
            let output = crate::docker::run_docker_output(
                &["info", "--format", "{{json .SecurityOptions}}"],
                "failed to ask Docker whether it runs rootless",
            )
            .map_err(|error| format!("{error:#}"))?;
            if !output.status.success() {
                return Err(format!(
                    "failed to ask Docker whether it runs rootless: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
            Ok(is_rootless_security_options(&String::from_utf8_lossy(
                &output.stdout,
            )))
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

/// Docker lists `name=rootless` among its security options in rootless mode.
fn is_rootless_security_options(options: &str) -> bool {
    options.contains("name=rootless")
}

#[cfg(test)]
mod tests {
    use super::is_rootless_security_options;

    #[test]
    fn each_engine_maps_the_host_user_its_own_way() {
        use super::user_args_for;
        // Unit tests model rootful Docker.
        assert_eq!(
            user_args_for(501, 20).expect("docker"),
            ["--user", "501:20"]
        );
        crate::docker::with_container_engine(crate::config::ContainerEngine::Podman, || {
            assert_eq!(
                user_args_for(501, 20).expect("podman"),
                ["--userns=keep-id", "--user", "501:20"]
            );
        });
    }

    #[test]
    fn rootless_docker_is_recognised_from_its_security_options() {
        assert!(is_rootless_security_options(
            r#"["name=seccomp,profile=builtin","name=rootless","name=cgroupns"]"#
        ));
        assert!(!is_rootless_security_options(
            r#"["name=seccomp,profile=builtin","name=cgroupns"]"#
        ));
        assert!(!is_rootless_security_options(""));
    }
}
