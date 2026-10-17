//! Primary command dispatch.
//!
//! This layer handles config/preset/profile/doctor/lock and delegates operational
//! commands to `operations`.

use anyhow::Result;

use crate::cli::args::{Cli, Commands, ConfigCommands, PresetCommands, ProfileCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config::Config;

mod operations;

use operations::dispatch_operation_commands;

/// Attempts to dispatch commands owned by the primary tree.
///
/// Returns `Some(result)` when the command was handled, otherwise `None` so the
/// secondary tree can attempt dispatch.
pub(super) fn dispatch_primary(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Init | Commands::Completions(_) => Some(Ok(())),
        Commands::Config(args) => Some(match args.command {
            Some(ConfigCommands::Migrate) => Err(anyhow::anyhow!(
                "`config migrate` is dispatched before config loading"
            )),
            Some(ConfigCommands::Validate { json }) => {
                handlers::handle_config_validate(config, json)
            }
            None => handlers::handle_config(config, &args.format),
        }),
        Commands::Preset(args) => Some(match &args.command {
            PresetCommands::List => {
                handlers::handle_preset_list();
                Ok(())
            }
            PresetCommands::Show { name, format } => handlers::handle_preset_show(name, format),
        }),
        Commands::Profile(args) => Some(match &args.command {
            ProfileCommands::List => {
                handlers::handle_profile_list();
                Ok(())
            }
            ProfileCommands::Show { name, format } => {
                handlers::handle_profile_show(config, name, format)
            }
        }),
        Commands::Doctor(args) => Some(handlers::handle_doctor(
            config,
            handlers::HandleDoctorOptions {
                format: &args.format,
                fix: args.fix,
                repro: args.repro,
                reachability: args.reachability,
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Lock(args) => Some(handlers::handle_lock(
            config,
            &args.command,
            context.quiet(),
            context.config_path(),
            context.project_root(),
        )),
        _ => dispatch_operation_commands(cli, config, context),
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::{
        args::{Cli, PresetCommands, ProfileCommands},
        dispatch::context::CliDispatchContext,
    };
    use crate::config::Config;
    use clap::Parser;

    fn sample_config() -> Config {
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        }
    }

    fn dispatch_primary_result(args: &[&str]) -> Option<Result<(), anyhow::Error>> {
        let cli = Cli::parse_from(args);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        super::dispatch_primary(&cli, &mut config, &context)
    }

    #[test]
    fn primary_dispatch_handles_meta_commands() {
        assert!(dispatch_primary_result(&["berth", "config"]).is_some());
        assert!(dispatch_primary_result(&["berth", "preset", "list"]).is_some());
        assert!(dispatch_primary_result(&["berth", "profile", "list"]).is_some());
        assert!(dispatch_primary_result(&["berth", "doctor"]).is_some());
        assert!(dispatch_primary_result(&["berth", "lock", "images"]).is_some());
    }

    #[test]
    fn primary_dispatch_falls_back_to_operations() {
        assert!(dispatch_primary_result(&["berth", "up"]).is_some());
    }

    #[test]
    fn primary_dispatch_does_not_handle_unknown() {
        assert!(dispatch_primary_result(&["berth", "about"]).is_none());
    }

    #[test]
    fn profile_and_preset_command_values() {
        let preset_command = PresetCommands::List;
        let profile_command = ProfileCommands::List;
        assert!(matches!(preset_command, PresetCommands::List));
        assert!(matches!(profile_command, ProfileCommands::List));
    }
}
