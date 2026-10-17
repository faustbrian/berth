//! Lifecycle dispatch for service operation commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config::Config;

use super::up;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Start(args) => Some(handlers::handle_start(
            config,
            handlers::HandleStartOptions {
                service: args.service(),
                kind: args.kind(),
                profile: args.profile(),
                wait: args.wait,
                no_wait: args.no_wait,
                wait_timeout: args.wait_timeout,
                pull_policy: up::pull_policy_from_arg(args.pull),
                force_recreate: args.force_recreate,
                open_after_start: !args.no_open && !context.non_interactive(),
                health_path: args.health_path(),
                include_project_deps: !args.no_deps,
                parallel: args.parallel,
                quiet: context.quiet(),
                no_color: context.no_color(),
                dry_run: context.dry_run(),
                repro: context.repro(),
                runtime_env: context.runtime_env(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Update(args) => Some(handlers::handle_update(
            config,
            handlers::HandleUpdateOptions {
                service: args.service(),
                kind: args.kind(),
                profile: args.profile(),
                force_recreate: args.force_recreate,
                no_build: args.no_build,
                wait: args.wait,
                wait_timeout: args.wait_timeout,
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Down(args) => Some(handlers::handle_down(
            config,
            handlers::HandleDownOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                include_project_deps: !args.no_deps,
                force: args.force,
                timeout: args.timeout,
                parallel: args.parallel,
                quiet: context.quiet(),
                no_color: context.no_color(),
                dry_run: context.dry_run(),
                runtime_env: context.runtime_env(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Recreate(args) => Some(handlers::handle_recreate(
            config,
            handlers::HandleRecreateOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                healthy: args.should_wait(),
                timeout: args.wait_timeout,
                random_ports: args.publish_all,
                save_ports: args.save_ports,
                env_output: args.env_output,
                parallel: args.parallel,
                quiet: context.quiet(),
                runtime_env: context.runtime_env(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Restart(args) => Some(handlers::handle_restart(
            config,
            handlers::HandleRestartOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                healthy: args.wait,
                timeout: args.wait_timeout,
                parallel: args.parallel,
            },
        )),
        Commands::Relabel(args) => Some(handlers::handle_relabel(
            config,
            handlers::HandleRelabelOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                wait: args.wait,
                wait_timeout: args.wait_timeout,
                parallel: args.parallel,
                config_path: context.config_path(),
                project_root: context.project_root(),
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
    fn service_ops_dispatches_start() {
        let result = dispatch_result(&["berth", "start"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_dispatches_update() {
        let result = dispatch_result(&["berth", "update"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_dispatches_down() {
        let result = dispatch_result(&["berth", "down"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_dispatches_recreate() {
        let result = dispatch_result(&["berth", "recreate"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_dispatches_restart() {
        let result = dispatch_result(&["berth", "restart"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_dispatches_relabel() {
        let result = dispatch_result(&["berth", "relabel"]);
        assert!(result.is_some());
    }

    #[test]
    fn service_ops_does_not_handle_other_commands() {
        assert!(dispatch_result(&["berth", "status"]).is_none());
    }
}
