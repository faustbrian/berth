//! Secondary command dispatch.
//!
//! This runs after primary dispatch and covers data commands, command execution
//! wrappers, and serve/swarm-oriented commands.

use anyhow::Result;

use crate::cli::args::Cli;
use crate::config;

mod data;
mod exec;
mod serve;

/// Dispatches commands handled by secondary subtrees.
///
/// This function intentionally returns `Ok(())` for commands already consumed by
/// earlier dispatch layers.
pub(super) fn dispatch_secondary(
    cli: &Cli,
    config: &mut config::Config,
    context: &super::context::CliDispatchContext<'_>,
) -> Result<()> {
    data::dispatch(cli, config, context)
        .or_else(|| exec::dispatch(cli, config, context))
        .or_else(|| serve::dispatch(cli, config, context))
        .unwrap_or(Ok(()))
}

#[cfg(test)]
mod tests {
    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;
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
    fn secondary_dispatch_returns_none_like_default_when_unhandled() {
        let cli = Cli::parse_from(["berth", "doctor"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        super::dispatch_secondary(&cli, &mut config, &context)
            .expect("dispatch_secondary should complete");
    }

    #[test]
    fn secondary_dispatch_covers_data_command() {
        let cli = Cli::parse_from(["berth", "status"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch_secondary(&cli, &mut config, &context);
        assert!(result.is_ok());
    }
}
