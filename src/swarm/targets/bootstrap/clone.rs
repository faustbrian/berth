//! Swarm target clone helpers.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::output::{self, LogLevel, Persistence};

pub(super) fn clone_missing_root(
    target_name: &str,
    target_root: &Path,
    git: &crate::config::SwarmGit,
    quiet: bool,
) -> Result<()> {
    if let Some(parent) = target_root.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directory '{}' for clone target",
                parent.display()
            )
        })?;
    }

    emit_clone_start(target_name, git, quiet);

    let status = build_clone_command(git)
        .arg(&git.repo)
        .arg(target_root)
        .status()
        .with_context(|| format!("failed to execute git clone for {}", target_root.display()))?;
    if status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "failed to clone '{}' into '{}' ({}); check your access to the repo (git's error \
         is above), or clone it there yourself and run the command again",
        git.repo,
        target_root.display(),
        status
    );
}

fn emit_clone_start(target_name: &str, git: &crate::config::SwarmGit, quiet: bool) {
    if quiet {
        return;
    }
    let branch_info = git
        .branch
        .as_deref()
        .map(|branch| format!(" (branch {branch})"))
        .unwrap_or_default();
    output::event(
        "swarm",
        LogLevel::Info,
        &format!(
            "Cloning target {target_name} from {}{}",
            git.repo, branch_info
        ),
        Persistence::Persistent,
    );
}

fn build_clone_command(git: &crate::config::SwarmGit) -> Command {
    let mut command = Command::new("git");
    command.arg("clone");
    if let Some(branch) = git.branch.as_deref() {
        command.args(["--branch", branch, "--single-branch"]);
    }
    command
        .stdin(Stdio::null())
        .stdout(output::child_stdout())
        .stderr(Stdio::inherit());
    // Unit tests clone fixture repositories: the developer's global git
    // configuration (such as a `core.hooksPath` with a Git LFS hook) must not
    // decide whether they pass.
    #[cfg(test)]
    command
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    command
}
