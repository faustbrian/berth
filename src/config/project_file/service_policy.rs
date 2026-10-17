//! Checks on what a committed `.berth.yaml` service may declare (ledger L003,
//! L011): project files live in worktrees agents edit, so they must not grant
//! a container control of the engine, and environment keys must be portable.

use anyhow::{Context, Result};
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

/// Where container engines put their sockets. Mounting one of them, or any
/// directory above one (`/`, `/var/run`, `~/.docker`), hands the container
/// the engine.
fn known_engine_sockets() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let mut sockets: Vec<PathBuf> = [
        "/var/run/docker.sock",
        "/run/docker.sock",
        "/run/podman/podman.sock",
        "/run/containerd/containerd.sock",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    if let Some(home) = &home {
        for relative in [
            ".docker/run/docker.sock",
            ".docker/desktop/docker.sock",
            ".colima/default/docker.sock",
            ".orbstack/run/docker.sock",
            ".local/share/containers/podman/machine/podman.sock",
        ] {
            sockets.push(home.join(relative));
        }
    }
    if let Some(runtime) = &runtime {
        sockets.push(runtime.join("docker.sock"));
        sockets.push(runtime.join("podman/podman.sock"));
    }
    if let Some(host) = std::env::var("DOCKER_HOST")
        .ok()
        .and_then(|host| host.strip_prefix("unix://").map(PathBuf::from))
    {
        sockets.push(host);
    }
    sockets
}

/// Validates an environment variable name: a letter or `_`, then letters,
/// digits, `_` or `.`. Dots are allowed because Elasticsearch and OpenSearch
/// read settings such as `discovery.type` from the environment.
///
/// # Errors
///
/// Returns an error naming the rejected key.
pub(crate) fn validate_env_key(service: &str, key: &str) -> Result<()> {
    let mut bytes = key.bytes();
    let valid = bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'.');
    if !valid {
        anyhow::bail!(
            "service '{service}' sets env key '{key}': keys start with a letter or `_` and contain only letters, digits, `_` and `.`"
        );
    }
    Ok(())
}

/// Refuses a bind mount that reaches a container engine socket: the socket
/// itself, a symlink to one, or a directory that contains one. Named
/// volumes are never host paths.
///
/// # Errors
///
/// Returns an error naming the volume.
pub(crate) fn validate_volume(service: &str, volume: &str, project_root: &Path) -> Result<()> {
    let source = volume.split(':').next().unwrap_or(volume);
    let Some(host) = host_path(source, project_root) else {
        return Ok(());
    };
    if reaches_engine_socket(&host, &known_engine_sockets())? {
        anyhow::bail!(
            "service '{service}' mounts '{source}', which reaches a container engine socket: that gives the container control of every container on the host, which a committed project file may not grant"
        );
    }
    Ok(())
}

/// The host path a bind mount names, or `None` for a named volume.
fn host_path(source: &str, project_root: &Path) -> Option<PathBuf> {
    if let Some(rest) = source.strip_prefix('~') {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        return Some(home.join(rest.trim_start_matches('/')));
    }
    if source.starts_with('/') {
        return Some(PathBuf::from(source));
    }
    let relative = project_root.join(source);
    // App services bind-mount a bare relative name from the checkout, so a
    // name that exists there (a committed symlink) is a host path too.
    (source.starts_with('.')
        || source.contains('/')
        || std::fs::symlink_metadata(&relative).is_ok())
    .then_some(relative)
}

/// Whether `host` (symlinks resolved) is a socket, or a directory holding one
/// of `sockets`. A path that does not exist yet is judged as written: Docker
/// would create an empty directory there.
fn reaches_engine_socket(host: &Path, sockets: &[PathBuf]) -> Result<bool> {
    let resolved = match std::fs::canonicalize(host) {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => host.to_path_buf(),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to resolve {}", host.display()));
        }
    };
    if std::fs::metadata(&resolved).is_ok_and(|metadata| metadata.file_type().is_socket()) {
        return Ok(true);
    }
    Ok(sockets.iter().any(|socket| {
        let socket = std::fs::canonicalize(socket).unwrap_or_else(|_| socket.clone());
        socket.starts_with(&resolved)
    }))
}

#[cfg(test)]
mod tests {
    use super::validate_env_key;

    #[test]
    fn env_keys_must_be_portable_names() {
        for key in ["APP_KEY", "_PRIVATE", "discovery.type", "a1"] {
            validate_env_key("app", key).unwrap_or_else(|error| panic!("{key}: {error}"));
        }
        for key in ["", "1A", "A-B", "A B", "A=B", "é"] {
            let error = validate_env_key("app", key).expect_err(key);
            assert!(error.to_string().contains(&format!("'{key}'")), "{error}");
        }
    }

    #[test]
    fn mounts_that_reach_an_engine_socket_are_refused() {
        use super::reaches_engine_socket;
        let root = std::env::temp_dir().join(format!("berth-socket-policy-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(root.join("engine")).expect("dir");
        let socket = root.join("engine/docker.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("socket");
        std::os::unix::fs::symlink(&socket, root.join("sock")).expect("symlink");
        std::fs::create_dir_all(root.join("project/storage")).expect("project");
        let known = [socket.clone()];

        assert!(reaches_engine_socket(&socket, &known).expect("socket"));
        assert!(reaches_engine_socket(&root.join("sock"), &[]).expect("symlink to a socket"));
        assert!(reaches_engine_socket(&root.join("engine"), &known).expect("its directory"));
        assert!(reaches_engine_socket(&root, &known).expect("an ancestor"));
        assert!(!reaches_engine_socket(&root.join("project/storage"), &known).expect("unrelated"));
        assert!(!reaches_engine_socket(&root.join("missing"), &known).expect("created by Docker"));
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn a_bare_name_that_is_a_committed_symlink_to_the_socket_is_refused() {
        use super::validate_volume;
        let root = std::env::temp_dir().join(format!("berth-socket-bare-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(root.join("project")).expect("project");
        let socket = root.join("engine.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("socket");
        std::os::unix::fs::symlink(&socket, root.join("project/sock")).expect("symlink");
        let error = validate_volume("app", "sock:/var/run/docker.sock", &root.join("project"))
            .expect_err("bare-name symlink");
        assert!(error.to_string().contains("engine socket"), "{error}");
        validate_volume("db", "data:/var/lib/data", &root.join("project")).expect("named volume");
        drop(std::fs::remove_dir_all(&root));
    }

    #[test]
    fn named_volumes_and_project_paths_are_not_host_sockets() {
        use super::validate_volume;
        let project = std::env::temp_dir();
        for volume in ["data:/var/lib/data", ".:/app", "./storage:/app/storage:ro"] {
            validate_volume("app", volume, &project)
                .unwrap_or_else(|error| panic!("{volume}: {error}"));
        }
        for volume in ["/:/host", "/var/run/docker.sock:/var/run/docker.sock"] {
            let error = validate_volume("app", volume, &project).expect_err(volume);
            assert!(error.to_string().contains("engine socket"), "{error}");
        }
    }
}
