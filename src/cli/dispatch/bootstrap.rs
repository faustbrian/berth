//! Bootstrap helpers used before regular command dispatch.
//!
//! These helpers handle commands that must run without reading project config
//! (init and shell completions), then build the effective `Config` for all other
//! commands.

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config::{self, Config};
use crate::output::{self, LogLevel, Persistence};

/// Handles setup-style commands that short-circuit normal dispatch.
///
/// Returns `Ok(true)` when a setup command was handled and no further dispatch
/// should run.
pub(super) fn handle_setup_commands(
    cli: &Cli,
    context: &super::context::CliDispatchContext<'_>,
) -> Result<bool> {
    if matches!(&cli.command, Commands::Init) {
        let path = config::init_config()?;
        if !context.quiet() {
            output::event(
                "init",
                LogLevel::Success,
                &format!("Created {}", path.display()),
                Persistence::Persistent,
            );
        }
        return Ok(true);
    }

    if let Commands::Completions(args) = &cli.command {
        let mut cmd = Cli::command();
        generate(args.shell, &mut cmd, "berth", &mut std::io::stdout());
        return Ok(true);
    }

    if let Commands::Daemon(args) = &cli.command {
        handlers::handle_daemon(args)?;
        return Ok(true);
    }

    Ok(false)
}

/// Loads config for this CLI invocation and applies `--env` overrides.
pub(super) fn load_config_for_cli(
    cli: &Cli,
    context: &super::context::CliDispatchContext<'_>,
) -> Result<Config> {
    let mut config =
        if cli.config.is_none() && cli.project_root.is_none() && context.runtime_env().is_none() {
            config::load_config()?
        } else {
            config::load_config_with(
                config::LoadConfigPathOptions::new(context.config_path(), context.project_root())
                    .with_runtime_env(context.runtime_env()),
            )?
        };
    if let Some(runtime_env) = context.runtime_env() {
        config::apply_runtime_env(&mut config, runtime_env)?;
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;
    use clap::Parser;

    use super::{handle_setup_commands, load_config_for_cli};

    fn minimal_config_dir() -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("berth-dispatch-bootstrap-{}", std::process::id()));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join(".berth.toml");
        fs::write(
            &config_path,
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write test config");
        dir
    }

    #[test]
    fn setup_commands_ignore_regular_commands() {
        let cli = Cli::parse_from(["berth", "about"]);
        let context = CliDispatchContext::from_cli(&cli);
        let handled = handle_setup_commands(&cli, &context).expect("handle setup");
        assert!(!handled);
    }

    #[test]
    fn load_config_uses_explicit_project_root() {
        let root = minimal_config_dir();
        let cli = Cli::parse_from([
            "berth",
            "--project-root",
            root.to_str().expect("root path"),
            "status",
        ]);
        let context = CliDispatchContext::from_cli(&cli);
        let config = load_config_for_cli(&cli, &context).expect("load config");
        assert_eq!(config.schema_version, 1);
        assert!(config.service.is_empty());
    }
}
