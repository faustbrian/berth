//! cli handlers app create cmd module.
//!
//! Contains cli handlers app create cmd logic used by Berth command workflows.

use crate::{cli, config};
use anyhow::Result;
use std::path::Path;

mod commands;

pub(crate) struct HandleAppCreateOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) no_migrate: bool,
    pub(crate) seed: bool,
    pub(crate) no_storage_link: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

pub(crate) fn handle_app_create(
    config: &config::Config,
    options: HandleAppCreateOptions<'_>,
) -> Result<()> {
    if config.project_type == config::ProjectType::Library {
        anyhow::bail!("`berth app-create` is only supported when project_type is \"project\"");
    }

    let runtime = cli::support::resolve_app_runtime_context(
        config,
        options.service,
        options.config_path,
        options.project_root,
    )?;
    let start_context = runtime.service_start_context();

    let setup_commands = commands::setup_commands();
    cli::support::run_service_commands(runtime.target, &setup_commands, &start_context)?;

    let mut post_setup_commands = Vec::new();
    if !options.no_storage_link {
        post_setup_commands.push(commands::storage_link_command());
    }

    if !options.no_migrate {
        post_setup_commands.push(commands::migrate_command());
    }

    if options.seed {
        post_setup_commands.push(commands::seed_command());
    }

    cli::support::run_service_commands(runtime.target, &post_setup_commands, &start_context)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{HandleAppCreateOptions, handle_app_create};
    use crate::config;

    #[test]
    fn handle_app_create_rejects_library_project_type() {
        let config = config::Config {
            schema_version: 1,
            project_type: config::ProjectType::Library,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        };

        let result = handle_app_create(
            &config,
            HandleAppCreateOptions {
                service: None,
                no_migrate: false,
                seed: false,
                no_storage_link: false,
                config_path: None,
                project_root: None,
            },
        );

        assert!(result.is_err());
        assert!(
            result
                .expect_err("library mode should reject app-create")
                .to_string()
                .contains("project_type is \"project\"")
        );
    }
}
