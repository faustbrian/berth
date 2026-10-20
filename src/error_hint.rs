//! A next step for errors whose cause is recognisable from their text.
//!
//! Many failures surface as the container engine's own stderr deep in an
//! error chain ("Cannot connect to the Docker daemon", "port is already
//! allocated"). Each site names what berth was doing; this adds, once, the
//! fix for the underlying cause, so every command shows it the same way.

/// The hint for an error chain rendered with `{:#}`, if its cause is known.
#[must_use]
pub(crate) fn hint(error: &str) -> Option<String> {
    let has = |needles: &[&str]| needles.iter().any(|needle| error.contains(needle));
    if let Some(binary) = missing_engine_binary(error) {
        let other = if binary == "docker" {
            "podman"
        } else {
            "docker"
        };
        return Some(format!(
            "`{binary}` is not on PATH: install it, or use {other} with `--engine {other}` (or \
             BERTH_ENGINE={other}); {}",
            crate::followup::doctor()
        ));
    }
    // The engine CLIs' own wording when their daemon or socket is down.
    if has(&[
        "Cannot connect to the Docker daemon",
        "error during connect",
        "Is the docker daemon running",
        "unable to connect to Podman socket",
        "failed to connect to the docker API",
    ]) {
        // How to start it differs: a desktop app or VM on macOS, a system
        // service on Linux.
        let start = if cfg!(target_os = "macos") {
            "start Docker Desktop (or `podman machine start`)"
        } else {
            "start it (`sudo systemctl start docker`; Podman needs its socket, `systemctl --user \
             start podman.socket`)"
        };
        return Some(format!(
            "the container engine is not running: {start}; {}",
            crate::followup::doctor()
        ));
    }
    if has(&["port is already allocated", "address already in use"]) {
        return Some(format!(
            "a host port is taken by another program: stop that program; for a service's \
             published port, {} publishes random host ports instead ({} shows them)",
            crate::followup::command("up -P"),
            crate::followup::command("port")
        ));
    }
    if has(&[
        "pull access denied",
        "manifest unknown",
        "repository does not exist",
    ]) {
        return Some(
            "the image could not be pulled: check the service's `image:` (or preset) in \
             .berth.yaml, and for a private registry log in with `docker login` (or `podman \
             login`)"
                .to_owned(),
        );
    }
    None
}

/// The engine CLI an error failed to start: berth's spawn context names it
/// (`Failed to execute docker … command`) and the OS reports it missing.
fn missing_engine_binary(error: &str) -> Option<&'static str> {
    if !error.contains("(os error 2)") {
        return None;
    }
    ["docker", "podman"]
        .into_iter()
        .find(|binary| error.contains(&format!("Failed to execute {binary} ")))
}

#[cfg(test)]
mod tests {
    use super::hint;

    #[test]
    fn an_unreachable_engine_says_to_start_it_and_run_doctor() {
        for text in [
            "failed to create Docker network x: Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
            "failed to list instance containers: error during connect: Get \"http://%2F%2F.%2Fpipe%2Fdocker_engine/v1.45/containers/json\"",
            "Error: unable to connect to Podman socket: failed to connect: dial unix /run/user/1000/podman/podman.sock: connect: no such file or directory",
            "failed to connect to the docker API at unix:///Users/me/.docker/run/docker.sock; check if the path is correct and if the daemon is running",
        ] {
            let hint = hint(text).unwrap_or_else(|| String::from("none"));
            assert!(
                hint.contains("not running") && hint.contains("berth doctor"),
                "{text}: {hint}"
            );
        }
    }

    #[test]
    fn a_missing_engine_cli_names_it_and_the_other_engine() {
        let hint =
            hint("Failed to execute podman run command: No such file or directory (os error 2)")
                .expect("hint");
        assert!(
            hint.contains("`podman`") && hint.contains("--engine"),
            "{hint}"
        );
    }

    #[test]
    fn a_taken_host_port_suggests_random_ports() {
        for text in [
            "Error response from daemon: driver failed programming external connectivity: Bind for 0.0.0.0:5432 failed: port is already allocated",
            "Error: rootlessport listen tcp 0.0.0.0:3306: bind: address already in use",
        ] {
            let hint = hint(text).expect("hint");
            assert!(hint.contains("berth up -P"), "{hint}");
        }
    }

    #[test]
    fn a_denied_image_pull_points_at_the_image_and_login() {
        for text in [
            "Failed to pull image acme/private:1: Error response from daemon: pull access denied for acme/private, repository does not exist or may require 'docker login'",
            "Failed to pull image redis:99: manifest unknown",
        ] {
            let hint = hint(text).expect("hint");
            assert!(
                hint.contains("`image:`") && hint.contains("login"),
                "{hint}"
            );
        }
    }

    #[test]
    fn unrelated_errors_get_no_hint() {
        assert_eq!(
            hint("service 'app' route 'EE' must be lowercase letters"),
            None
        );
        assert_eq!(hint("PHP Fatal error: Uncaught Error"), None);
    }
}
