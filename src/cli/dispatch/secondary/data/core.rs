//! Secondary data dispatch for core diagnostics commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands};
use crate::cli::handlers;
use crate::config;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &crate::cli::dispatch::context::CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::About(args) => Some(handlers::handle_about(
            config,
            &args.format,
            context.runtime_env(),
            context.config_path(),
            context.project_root(),
        )),
        Commands::Restore(args) => Some(handlers::handle_restore(
            config,
            handlers::HandleRestoreOptions {
                service: args.service(),
                file: args.file.as_ref(),
                reset: args.reset,
                migrate: args.migrate,
                schema_dump: args.schema_dump,
                gzip: args.gzip,
                project_root: context.project_root(),
                config_path: context.config_path(),
            },
        )),
        Commands::Dump(args) => Some(handlers::handle_dump(
            config,
            handlers::HandleDumpOptions {
                service: args.service(),
                file: args.file.as_ref(),
                stdout: args.stdout,
                gzip: args.gzip,
            },
        )),
        Commands::Ps(args) => Some(handlers::handle_status(
            config,
            &args.format,
            args.kind(),
            args.driver(),
        )),
        Commands::Health(args) => Some(handlers::handle_health(
            config,
            handlers::HandleHealthOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                format: &args.format,
                timeout: args.timeout,
                interval: args.interval,
                retries: args.retries,
                parallel: args.parallel,
            },
        )),
        Commands::Env(args) => Some(handlers::handle_env(
            config,
            handlers::HandleEnvOptions {
                command: args.command.as_ref(),
                service: args.service(),
                json: args.json,
                writes_dotenv: args.kind.is_some()
                    || args.env_file.is_some()
                    || args.sync
                    || args.purge
                    || args.create_missing,
                quiet: context.quiet(),
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Logs(args) => Some(handlers::handle_logs(
            config,
            handlers::HandleLogsOptions {
                service: args.service(),
                services: args.services(),
                kind: args.kind(),
                profile: args.profile(),
                all: args.all,
                follow: args.follow,
                tail: args.tail,
                since: args.since.as_deref(),
                until: args.until.as_deref(),
                timestamps: args.timestamps,
                prefix: args.prefix,
                access: args.access,
            },
        )),
        Commands::Pull(args) => Some(handlers::handle_pull(
            config,
            args.service(),
            args.services(),
            args.kind(),
            args.profile(),
            args.parallel,
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
    fn data_core_dispatches_about() {
        assert!(dispatch_result(&["berth", "about"]).is_some());
    }

    #[test]
    fn data_core_dispatches_restore() {
        assert!(dispatch_result(&["berth", "restore", "--file", "/tmp/file.sql"]).is_some());
    }

    #[test]
    fn data_core_dispatches_dump() {
        assert!(dispatch_result(&["berth", "dump", "--file", "/tmp/dump.sql"]).is_some());
    }

    #[test]
    fn data_core_dispatches_list_as_status() {
        assert!(dispatch_result(&["berth", "ps"]).is_some());
    }

    #[test]
    fn data_core_dispatches_health() {
        assert!(dispatch_result(&["berth", "health"]).is_some());
    }

    #[test]
    fn data_core_dispatches_env() {
        assert!(dispatch_result(&["berth", "env"]).is_some());
    }

    #[test]
    fn data_core_dispatches_logs() {
        assert!(dispatch_result(&["berth", "logs"]).is_some());
    }

    #[test]
    fn data_core_dispatches_pull() {
        assert!(dispatch_result(&["berth", "pull"]).is_some());
    }

    #[test]
    fn data_core_dispatches_none_for_setup_command() {
        assert!(dispatch_result(&["berth", "setup"]).is_none());
    }
}
