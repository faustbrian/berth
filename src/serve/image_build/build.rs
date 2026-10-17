//! Docker build execution for derived serve images.

use anyhow::{Context, Result};

use super::docker_cmd::docker_status;
use crate::output::{self, LogLevel, Persistence};

struct BuildContextDir {
    path: std::path::PathBuf,
}

impl BuildContextDir {
    fn create() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "berth-serve-build-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&path)
            .with_context(|| format!("failed to create {}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for BuildContextDir {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.path));
    }
}

/// Builds a derived image from in-memory Dockerfile content.
///
/// Writes build context into a temporary directory, runs `docker build`, then
/// removes the temporary directory.
pub(super) fn build_derived_image(tag: &str, dockerfile: &str) -> Result<()> {
    let build_context = BuildContextDir::create()?;
    let dockerfile_path = build_context.path.join("Dockerfile");
    std::fs::write(&dockerfile_path, dockerfile)
        .with_context(|| format!("failed to write {}", dockerfile_path.display()))?;

    output::event(
        "build",
        LogLevel::Info,
        &format!("Waiting for Docker build slot for derived serve image {tag}"),
        Persistence::Persistent,
    );
    let dockerfile_arg = dockerfile_path.to_string_lossy().into_owned();
    let context_arg = build_context.path.to_string_lossy().into_owned();
    let status = crate::docker::with_scheduled_docker_op(
        crate::docker::DockerOpClass::Build,
        "docker-build-derived-image",
        || {
            output::event(
                "build",
                LogLevel::Info,
                &format!("Building derived serve image {tag}"),
                Persistence::Persistent,
            );
            docker_status(
                &["build", "-t", tag, "-f", &dockerfile_arg, &context_arg],
                &crate::docker::runtime_command_error_context("build"),
            )
        },
    )?;

    if status.success() {
        return Ok(());
    }

    anyhow::bail!("failed to build derived serve image {tag}");
}
