//! cli handlers env cmd module.
//!
//! Contains cli handlers env cmd logic used by Berth command workflows.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::cli::args::EnvCommands;
use crate::{cli, config};

mod generate;
mod managed;
mod update;

use managed::ManagedEnvUpdateOptions;
use update::ServiceEnvUpdateOptions;

pub(crate) struct HandleEnvOptions<'a> {
    pub(crate) command: Option<&'a EnvCommands>,
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) env_file: &'a Option<PathBuf>,
    pub(crate) sync: bool,
    pub(crate) purge: bool,
    pub(crate) persist_runtime: bool,
    pub(crate) create_missing: bool,
    pub(crate) quiet: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
    pub(crate) runtime_env: Option<&'a str>,
}

pub(crate) fn handle_env(config: &mut config::Config, options: HandleEnvOptions<'_>) -> Result<()> {
    if let Some(EnvCommands::Generate { output }) = options.command {
        return generate::handle_generate_env(config, output, options.quiet);
    }

    let env_path = cli::support::default_env_path(
        options.config_path,
        options.project_root,
        options.env_file.as_deref(),
        options.runtime_env,
    )?;

    if options.sync || options.purge || options.persist_runtime {
        return managed::handle_managed_env_update(
            config,
            ManagedEnvUpdateOptions {
                service: options.service,
                kind: options.kind,
                env_path: &env_path,
                sync: options.sync,
                purge: options.purge,
                persist_runtime: options.persist_runtime,
                create_missing: options.create_missing,
                quiet: options.quiet,
                config_path: options.config_path,
                project_root: options.project_root,
            },
        );
    }

    update::handle_service_env_update(
        config,
        ServiceEnvUpdateOptions {
            service: options.service,
            kind: options.kind,
            env_path: &env_path,
            create_missing: options.create_missing,
            quiet: options.quiet,
        },
    )
}
