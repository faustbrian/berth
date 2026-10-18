//! Lifecycle dispatch for `up` and `apply`.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::config::Config;

use super::up;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Up(args) => Some(up::dispatch_up(
            config,
            context,
            up::DispatchUpOptions {
                service: args.service(),
                kind: args.kind(),
                profile: args.profile(),
                wait: args.wait,
                no_wait: args.no_wait,
                wait_timeout: args.wait_timeout,
                pull: args.pull,
                force_recreate: args.force_recreate,
                publish_all: args.publish_all,
                no_publish_all: args.no_publish_all,
                port_strategy: args.port_strategy,
                port_seed: args.port_seed(),
                env_output: args.env_output,
                include_project_deps: !args.no_deps,
                seed: args.seed,
                parallel: args.parallel,
                json: args.json,
            },
        )),
        Commands::Apply(args) => Some(up::dispatch_up(
            config,
            context,
            up::DispatchUpOptions {
                service: None,
                kind: None,
                profile: None,
                wait: true,
                no_wait: false,
                wait_timeout: None,
                pull: crate::cli::args::PullPolicyArg::Missing,
                force_recreate: false,
                publish_all: false,
                no_publish_all: false,
                port_strategy: crate::cli::args::PortStrategyArg::Random,
                port_seed: None,
                env_output: false,
                include_project_deps: !args.no_deps,
                seed: true,
                parallel: 1,
                json: false,
            },
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

    fn dispatch_result(args: &[&str]) -> Option<Result<(), anyhow::Error>> {
        let cli = Cli::parse_from(args);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        super::dispatch(&cli, &mut config, &context)
    }

    #[test]
    fn up_apply_dispatches_up_command() {
        let result = dispatch_result(&["berth", "up", "--service", "api"]);
        assert!(result.is_some());
    }

    #[test]
    fn up_apply_dispatches_apply_command() {
        let result = dispatch_result(&["berth", "apply"]);
        assert!(result.is_some());
    }

    #[test]
    fn up_apply_does_not_handle_other_commands() {
        assert!(dispatch_result(&["berth", "down"]).is_none());
    }
}
