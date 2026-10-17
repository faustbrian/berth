//! Secondary data dispatch for docker-ops style commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    _context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Top(args) => Some(handlers::handle_top(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            &args.args,
        )),
        Commands::Stats(args) => Some(handlers::handle_stats(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.no_stream,
            args.format(),
        )),
        Commands::Inspect(args) => Some(handlers::handle_inspect(
            config,
            handlers::HandleInspectOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                format: args.format(),
                output: &args.output,
                json: args.json,
                size: args.size,
                object_type: args.object_type(),
            },
        )),
        Commands::Attach(args) => Some(handlers::handle_attach(
            config,
            handlers::HandleAttachOptions {
                service: args.service(),
                no_stdin: args.no_stdin,
                sig_proxy: args.sig_proxy,
                detach_keys: args.detach_keys(),
            },
        )),
        Commands::Cp(args) => Some(handlers::handle_cp(
            config,
            handlers::HandleCpOptions {
                source: &args.source,
                destination: &args.destination,
                follow_link: args.follow_link,
                archive: args.archive,
            },
        )),
        Commands::Kill(args) => Some(handlers::handle_kill(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.signal(),
            args.parallel,
        )),
        Commands::Pause(args) => Some(handlers::handle_pause(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.parallel,
        )),
        Commands::Unpause(args) => Some(handlers::handle_unpause(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.parallel,
        )),
        Commands::Wait(args) => Some(handlers::handle_wait(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.condition(),
            args.parallel,
        )),
        Commands::Events(args) => Some(handlers::handle_events(
            config,
            handlers::HandleEventsOptions {
                service: args.service(),
                kind: args.kind(),
                since: args.since(),
                until: args.until(),
                format: args.format(),
                json: args.json,
                all: args.all,
                allow_empty: args.allow_empty,
                filter: &args.filter,
            },
        )),
        Commands::Port(args) => Some(handlers::handle_port(
            config,
            handlers::HandlePortOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                format: &args.format,
                json: args.json,
                private_port: args.private_port(),
            },
        )),
        Commands::Prune(args) => Some(handlers::handle_prune(
            config,
            handlers::HandlePruneOptions {
                service: args.service(),
                kind: args.kind(),
                parallel: args.parallel,
                all: args.all,
                force: args.force,
                filter: &args.filter,
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
    fn docker_ops_dispatches_top() {
        assert!(dispatch_result(&["berth", "top"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_stats() {
        assert!(dispatch_result(&["berth", "stats"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_inspect() {
        assert!(dispatch_result(&["berth", "inspect"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_attach() {
        assert!(dispatch_result(&["berth", "attach"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_cp() {
        assert!(dispatch_result(&["berth", "cp", "src", "dst"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_kill() {
        assert!(dispatch_result(&["berth", "kill"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_pause() {
        assert!(dispatch_result(&["berth", "pause"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_unpause() {
        assert!(dispatch_result(&["berth", "unpause"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_wait() {
        assert!(dispatch_result(&["berth", "wait"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_events() {
        assert!(dispatch_result(&["berth", "events"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_port() {
        assert!(dispatch_result(&["berth", "port"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_prune() {
        assert!(dispatch_result(&["berth", "prune"]).is_some());
    }

    #[test]
    fn docker_ops_dispatches_none_for_core_command() {
        assert!(dispatch_result(&["berth", "about"]).is_none());
    }
}
