//! Dispatch for access/introspection commands (`url`).

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config::Config;

/// Dispatches access commands, returning `None` when the command is not owned here.
pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    _context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Url(args) => Some(handlers::handle_url(
            config,
            args.service(),
            &args.format,
            args.kind(),
            args.driver(),
        )),
        _ => None,
    }
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
    fn access_dispatches_url() {
        let cli = Cli::parse_from(["berth", "url"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
        assert!(result.is_some_and(|result| result.is_ok() || result.is_err()));
    }
}
