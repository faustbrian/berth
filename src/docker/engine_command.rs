//! The container engine's CLI as a process, refused early when it is not
//! installed.

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A `Command` for the active engine CLI. A missing binary otherwise
/// surfaces as a bare "No such file or directory (os error 2)" behind
/// whatever berth was doing, so this names it and the way out first.
///
/// # Errors
///
/// Returns an error when the engine CLI is not on `PATH`.
pub(crate) fn engine_command() -> Result<Command> {
    ensure_engine_installed()?;
    Ok(Command::new(super::docker_command()))
}

/// Fails, naming the missing engine CLI and the other engine, when it is not
/// on `PATH`. Callers check this right before they spawn, so building a
/// command's arguments never needs an engine installed.
///
/// # Errors
///
/// Returns an error when the engine CLI is not on `PATH`.
pub(crate) fn ensure_engine_installed() -> Result<()> {
    let program = super::docker_command();
    // Without `PATH` the OS searches its default path at spawn, which this
    // check cannot mirror, so the spawn reports a missing binary instead.
    let Some(path) = std::env::var_os("PATH") else {
        return Ok(());
    };
    if resolve(&program, Some(&path)).is_none() {
        let other = if super::container_engine() == crate::config::ContainerEngine::Docker {
            "podman"
        } else {
            "docker"
        };
        anyhow::bail!(
            "`{program}` was not found on PATH; install it, or use {other} instead with `--engine \
             {other}` (or BERTH_ENGINE={other}); {}",
            crate::followup::doctor()
        );
    }
    Ok(())
}

/// Where `program` would be found: itself when it names a path, else the
/// first `PATH` entry holding it.
fn resolve(program: &str, path: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if program.contains(std::path::MAIN_SEPARATOR) {
        let program = Path::new(program);
        return program.is_file().then(|| program.to_path_buf());
    }
    std::env::split_paths(path?)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn a_binary_is_found_on_path_or_by_its_own_path() {
        let dir = crate::test_support::temp_root()
            .join(format!("berth-engine-command-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(dir.join("podman"), "").expect("binary");
        let path = std::ffi::OsString::from(dir.as_os_str());
        assert_eq!(resolve("podman", Some(&path)), Some(dir.join("podman")));
        assert_eq!(resolve("docker", Some(&path)), None);
        assert_eq!(resolve("docker", None), None);
        let absolute = dir.join("podman");
        assert_eq!(
            resolve(absolute.to_str().expect("utf8"), None),
            Some(absolute.clone())
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
