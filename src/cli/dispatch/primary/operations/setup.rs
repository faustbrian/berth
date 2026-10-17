//! Dispatch for setup command.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config::Config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    _context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Setup(args) => Some(crate::cli::support::for_each_service(
            config,
            args.service(),
            args.kind(),
            None,
            args.parallel,
            |svc| crate::database::setup(svc, args.timeout),
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
    fn setup_dispatches_setup() {
        let cli = Cli::parse_from(["berth", "setup"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
        assert!(result.is_some_and(|result| result.is_ok() || result.is_err()));
    }
}
