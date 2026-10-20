//! Renews the current instance's lease and reaps abandoned instances before
//! a command runs (criterion 4: crash recovery without a daemon).

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::config::Config;

/// Renews the lease of the instance the command runs in (setting the TTL
/// `up --ttl` asks for) and reaps other instances whose lease lapsed or
/// whose checkout is gone. Commands that never touch Docker skip both.
///
/// # Errors
///
/// Returns an error for an invalid `--ttl`, or when the lease cannot be
/// written or reaping fails.
pub(super) fn renew_and_reap(
    cli: &Cli,
    config: &Config,
    early: Option<&super::early_lock::EarlyLock>,
) -> Result<Option<ActiveLease>> {
    let (ttl, owner) = match &cli.command {
        Commands::Up(args) => (
            args.ttl
                .as_deref()
                .map(crate::instance::parse_ttl)
                .transpose()?
                .unwrap_or(crate::instance::TtlChange::Keep),
            args.owner_pid
                .map(crate::instance::owner_process)
                .transpose()?,
        ),
        _ => (crate::instance::TtlChange::Keep, None),
    };
    if !leases(&cli.command) {
        return Ok(None);
    }
    let identity = config.service.iter().find_map(|service| {
        let runtime = &service.runtime;
        Some((
            runtime.project.clone()?,
            runtime.instance.clone()?,
            runtime.root.clone()?,
        ))
    });
    let Some((project, instance, root)) = identity else {
        reap_in_passing(None);
        return Ok(None);
    };
    match early {
        Some(early) if early.project == project && early.instance == instance => {}
        Some(early) => anyhow::bail!(
            "berth marked {}/{} in use but loaded {project}/{instance}",
            early.project,
            early.instance
        ),
        None => anyhow::bail!("berth did not mark {project}/{instance} in use before loading it"),
    }
    // Only `up` establishes the instance anew and takes over from a dead
    // owner; any other command keeps the recorded owner, so a command that
    // outlives its killed agent never stops the instance from being reaped.
    let take_over = matches!(cli.command, Commands::Up(_));
    crate::instance::renew_lease(&project, &instance, &root, ttl, owner, take_over)?;
    reap_in_passing(Some((&project, &instance)));
    Ok(Some(ActiveLease {
        project,
        instance,
        root,
    }))
}

/// The instance a command runs in (`dispatch` holds it in use until the
/// command ends).
pub(super) struct ActiveLease {
    project: String,
    instance: String,
    root: std::path::PathBuf,
}

impl ActiveLease {
    /// Renews the lease again as the command ends, so a TTL counts from the
    /// last command, not the start of a long one. A purge that deleted the
    /// instance's state is not undone.
    ///
    /// # Errors
    ///
    /// Returns an error when the lease cannot be written.
    pub(super) fn finish(self) -> Result<()> {
        if !crate::home::home_path(
            &std::path::Path::new("instances")
                .join(&self.project)
                .join(&self.instance),
        )?
        .exists()
        {
            return Ok(());
        }
        crate::instance::renew_lease(
            &self.project,
            &self.instance,
            &self.root,
            crate::instance::TtlChange::Keep,
            None,
            false,
        )
        .map(drop)
    }
}

/// Reaps on behalf of the command. Another instance's leftovers are not the
/// command's failure: one that cannot be deleted must never block every
/// agent's commands (8.x FI-53), so a failure is a warning here and an
/// error only for `berth gc`, which reaps on purpose.
pub(super) fn reap_in_passing(keep: Option<(&str, &str)>) {
    // Unit tests never reap the developer's real Docker and berth home.
    if cfg!(test) {
        return;
    }
    if let Err(error) = crate::shared::best_effort(|| crate::instance::reap(false, keep)) {
        crate::output::event(
            "reap",
            crate::output::LogLevel::Warn,
            &format!(
                "{error:#}; {} retries and reports it",
                crate::followup::global("gc")
            ),
            crate::output::Persistence::Persistent,
        );
    }
}

/// Whether `command` leases its instance (and so marks it in use). Unit
/// tests drive dispatch against the developer's real Docker and berth home,
/// which must never be reaped; the integration tests cover reaping through
/// the binary with a fake Docker.
pub(super) fn leases(command: &Commands) -> bool {
    !cfg!(test) && touches_docker(command) && !crate::docker::is_dry_run()
}

/// Commands that only read or write project files.
fn touches_docker(command: &Commands) -> bool {
    !matches!(
        command,
        Commands::Init
            | Commands::Config(_)
            | Commands::Preset(_)
            | Commands::Profile(_)
            | Commands::Env(_)
            | Commands::Lock(_)
            | Commands::Completions(_)
            | Commands::Daemon(_)
    ) && !matches!(command, Commands::Ls(args) if args.services)
}

/// Prints `up --json`'s failure document for an error raised before `up`
/// itself runs.
///
/// # Errors
///
/// Returns an error when the document cannot be rendered.
#[allow(clippy::print_stdout)] // `--json` prints its one document on stdout
pub(super) fn print_failure_document(config: &Config, error: &anyhow::Error) -> Result<()> {
    let document = crate::cli::handlers::failed_report(config, error);
    println!(
        "{}",
        serde_json::to_string_pretty(&document)
            .map_err(|render| anyhow::anyhow!("failed to render the failure document: {render}"))?
    );
    Ok(())
}
