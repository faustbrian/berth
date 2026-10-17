//! start command bootstrap helpers.

use anyhow::Result;
use std::path::Path;

use crate::cli::handlers::log;
use crate::{cli, config};

pub(super) struct StartBootstrapOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) quiet: bool,
}

pub(super) fn run_start_bootstrap(
    config: &config::Config,
    options: StartBootstrapOptions<'_>,
) -> Result<()> {
    let selected =
        cli::support::resolve_up_services(config, options.service, options.kind, options.profile)?;
    let Some(target) = select_bootstrap_target(&selected) else {
        log::info_if_not_quiet(
            options.quiet,
            "start",
            "Skipping Laravel bootstrap because no web app target was selected",
        );
        return Ok(());
    };

    let runtime = cli::support::resolve_project_runtime_context(
        config,
        options.config_path,
        options.project_root,
    )?;
    // No `key:generate`: it writes `APP_KEY` into the host `.env` through
    // the bind mount, and berth never writes `.env`. A missing key is
    // injected as the instance's managed `APP_KEY` instead.
    let bootstrap_commands = vec![storage_link_command(), migrate_command()];
    let start_context = runtime.service_start_context();

    cli::support::run_service_commands(target, &bootstrap_commands, &start_context)?;

    Ok(())
}

pub(super) fn select_bootstrap_target<'a>(
    selected: &[&'a config::ServiceConfig],
) -> Option<&'a config::ServiceConfig> {
    selected.iter().copied().find(|svc| {
        svc.kind == config::Kind::App
            && svc.driver == config::Driver::Frankenphp
            && svc.command.is_none()
    })
}

fn storage_link_command() -> Vec<String> {
    cli::support::build_artisan_subcommand("storage:link", &["--ansi", "--force"])
}

fn migrate_command() -> Vec<String> {
    cli::support::build_artisan_subcommand("migrate", &["--isolated", "--ansi", "--force"])
}
