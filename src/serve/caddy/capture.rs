//! Temporary-file command capture for Caddy subprocesses.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Output;

pub(super) struct CommandCapture {
    stdout_path: PathBuf,
    stderr_path: PathBuf,
}

impl CommandCapture {
    pub(super) fn new() -> Result<Self> {
        let temp_dir = std::env::temp_dir();
        let unique = format!(
            "berth-caddy-capture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        let stdout_path = temp_dir.join(format!("{unique}.stdout"));
        let stderr_path = temp_dir.join(format!("{unique}.stderr"));
        std::fs::File::create(&stdout_path)
            .with_context(|| format!("failed to create {}", stdout_path.display()))?;
        std::fs::File::create(&stderr_path)
            .with_context(|| format!("failed to create {}", stderr_path.display()))?;

        Ok(Self {
            stdout_path,
            stderr_path,
        })
    }

    pub(super) fn stdout_stdio(&self) -> Result<std::process::Stdio> {
        let file = std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(&self.stdout_path)
            .with_context(|| format!("failed to open {}", self.stdout_path.display()))?;
        Ok(std::process::Stdio::from(file))
    }

    pub(super) fn stderr_stdio(&self) -> Result<std::process::Stdio> {
        let file = std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(&self.stderr_path)
            .with_context(|| format!("failed to open {}", self.stderr_path.display()))?;
        Ok(std::process::Stdio::from(file))
    }

    pub(super) fn finish(self, status: std::process::ExitStatus) -> Result<Output> {
        let stdout = read_capture_file(&self.stdout_path)?;
        let stderr = read_capture_file(&self.stderr_path)?;
        drop(std::fs::remove_file(&self.stdout_path));
        drop(std::fs::remove_file(&self.stderr_path));

        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

fn read_capture_file(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))
}
