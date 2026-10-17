//! Dispatch for command-execution style commands.
//!
//! Covers `exec`, `artisan`, `composer`, `node`, `task`, and `app-create`.

use anyhow::Result;

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config;

mod wrappers;

/// Attempts to dispatch a command in the execution group.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    wrappers::dispatch(cli, config, context)
}

#[cfg(test)]
mod tests {
    use crate::cli::{args::Cli, dispatch::context::CliDispatchContext};
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

    #[test]
    fn secondary_exec_delegates_wrapper_commands() {
        let cli = Cli::parse_from(["berth", "exec"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }

    #[test]
    fn secondary_exec_rejects_unrelated_commands() {
        let cli = Cli::parse_from(["berth", "about"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_none());
    }
}
