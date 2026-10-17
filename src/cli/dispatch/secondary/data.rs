//! Dispatch for data/diagnostics commands.
//!
//! Includes commands like `about`, `restore`, `dump`, `health`, `env`, `logs`,
//! and docker data operations.

use anyhow::Result;

use crate::cli::args::Cli;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config;

mod core;
mod docker_ops;

/// Attempts to dispatch a command in the data/diagnostics group.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    core::dispatch(cli, config, context).or_else(|| docker_ops::dispatch(cli, config, context))
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
    fn data_module_dispatch_prefers_core() {
        let cli = Cli::parse_from(["berth", "about"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }

    #[test]
    fn data_module_dispatch_falls_back_to_docker_ops() {
        let cli = Cli::parse_from(["berth", "top"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }
}
