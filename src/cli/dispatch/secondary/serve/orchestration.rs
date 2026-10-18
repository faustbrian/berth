//! Secondary serve dispatch for swarm/serve orchestration commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands, ShareCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Swarm(args) => Some(handlers::handle_swarm(
            config,
            handlers::HandleSwarmOptions {
                command: &args.command,
                only: &args.only,
                no_deps: args.no_deps,
                force: args.force,
                parallel: args.parallel,
                fail_fast: args.fail_fast,
                port_strategy: args.port_strategy,
                port_seed: args.port_seed(),
                env_output: args.env_output,
                quiet: context.quiet(),
                no_color: context.no_color(),
                dry_run: context.dry_run(),
                repro: context.repro(),
                runtime_env: context.runtime_env(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Serve(args) => Some(handlers::handle_serve(
            config,
            handlers::HandleServeOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                recreate: args.recreate,
                detached: args.detached,
                env_output: args.env_output,
                trust_container_ca: args.trust_container_ca,
                runtime_env: context.runtime_env(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Share(args) => Some(dispatch_share_command(config, &args.command)),
        Commands::EnvScrub(args) => Some(handlers::handle_env_scrub(
            context.config_path(),
            context.project_root(),
            args.env_file(),
            context.runtime_env(),
        )),
        _ => None,
    }
}

fn dispatch_share_command(config: &config::Config, command: &ShareCommands) -> Result<()> {
    match command {
        ShareCommands::Start(share_args) => handlers::handle_share_start(
            config,
            handlers::HandleShareStartOptions {
                service: share_args.service(),
                selection: share_args.provider_selection().into(),
                detached: share_args.detached,
                timeout: share_args.timeout,
                json: share_args.json,
            },
        ),
        ShareCommands::Status(share_args) => handlers::handle_share_status(
            share_args.service(),
            share_args.provider_selection().into(),
            share_args.json,
        ),
        ShareCommands::Stop(share_args) => {
            handlers::handle_share_stop(handlers::HandleShareStopOptions {
                service: share_args.service(),
                selection: share_args.provider_selection().into(),
                all: share_args.all,
                json: share_args.json,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::args::{Cli, Commands, ShareCommands};
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

    fn parse_share_command(args: &[&str]) -> ShareCommands {
        let cli = Cli::parse_from(args);
        match cli.command {
            Commands::Share(share_args) => share_args.command,
            _ => unreachable!("command must be share"),
        }
    }

    #[test]
    fn dispatch_share_command_forwards_start_status_stop() {
        // Share state lives under HOME; another test's sessions in the
        // developer's real home made this test fail intermittently.
        let home = crate::test_support::temp_root().join(format!(
            "berth-share-dispatch-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        drop(std::fs::remove_dir_all(&home));
        std::fs::create_dir_all(&home).expect("temporary home");
        let config = sample_config();
        crate::share::with_test_state_home(&home, || {
            let command = parse_share_command(&[
                "berth",
                "share",
                "start",
                "--service",
                "api",
                "--cloudflare",
            ]);
            assert!(super::dispatch_share_command(&config, &command).is_err());

            let command = parse_share_command(&["berth", "share", "status"]);
            assert!(super::dispatch_share_command(&config, &command).is_ok());

            let command = parse_share_command(&["berth", "share", "stop", "--all"]);
            assert!(super::dispatch_share_command(&config, &command).is_ok());
        });
        std::fs::remove_dir_all(&home).expect("cleanup");
    }

    #[test]
    fn serve_dispatch_handles_swarm_command() {
        let cli = Cli::parse_from(["berth", "swarm", "up"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }

    #[test]
    fn serve_dispatch_handles_serve_command() {
        let cli = Cli::parse_from(["berth", "serve"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }

    #[test]
    fn serve_dispatch_forwards_share_command() {
        let cli = Cli::parse_from(["berth", "share", "status"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }

    #[test]
    fn serve_dispatch_handles_env_scrub() {
        let cli = Cli::parse_from(["berth", "env-scrub", "--env-file", ".env"]);
        let context = CliDispatchContext::from_cli(&cli);
        let mut config = sample_config();
        let result = super::dispatch(&cli, &mut config, &context);
        assert!(result.is_some());
    }
}
