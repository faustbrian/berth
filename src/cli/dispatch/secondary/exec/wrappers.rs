//! Secondary exec dispatch for wrapper-style commands.

use anyhow::Result;

use crate::cli::args::{Cli, Commands, TaskCommands, TaskDepsCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::handlers;
use crate::config;
use crate::javascript::JavaScriptRuntime;

pub(super) fn dispatch(
    cli: &Cli,
    config: &mut config::Config,
    context: &CliDispatchContext<'_>,
) -> Option<Result<()>> {
    match &cli.command {
        Commands::Exec(args) => Some(handlers::handle_exec(
            config,
            handlers::HandleExecOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
            },
        )),
        Commands::Artisan(args) => Some(handlers::handle_artisan(
            config,
            handlers::HandleArtisanOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                non_interactive: context.non_interactive(),
                browser: args.browser,
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
            },
        )),
        Commands::Composer(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("composer"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &["list"],
            },
        )),
        Commands::Phpstan(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("phpstan"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Ecs(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("ecs"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::PhpCsFixer(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("php-cs-fixer"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Psalm(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("psalm"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Pint(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("pint"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Pest(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("pest"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Phpunit(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("phpunit"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Rector(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("rector"),
                runtime: None,
                package_manager: None,
                version_manager: None,
                node_version: None,
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Node(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: None,
                runtime: Some(JavaScriptRuntime::Node),
                package_manager: args.package_manager,
                version_manager: args.version_manager,
                node_version: args.node_version.as_deref(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Bun(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("bun"),
                runtime: Some(JavaScriptRuntime::Bun),
                package_manager: None,
                version_manager: None,
                node_version: args.bun_version.as_deref(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Deno(args) => Some(handlers::handle_package_manager_command(
            config,
            handlers::HandlePackageManagerCommandOptions {
                service: args.service(),
                kind: args.kind,
                profile: args.profile(),
                command_bin: Some("deno"),
                runtime: Some(JavaScriptRuntime::Deno),
                package_manager: None,
                version_manager: None,
                node_version: args.deno_version.as_deref(),
                non_interactive: context.non_interactive(),
                tty: args.tty,
                no_tty: args.no_tty,
                command: &args.command,
                config_path: context.config_path(),
                project_root: context.project_root(),
                default_command: &[],
            },
        )),
        Commands::Task(args) => match &args.command {
            TaskCommands::Deps(args) => match &args.command {
                TaskDepsCommands::Bump(args) => Some(handlers::handle_task_deps_workflow(
                    config,
                    handlers::HandleTaskDepsWorkflowOptions {
                        action: handlers::TaskDependencyAction::Bump,
                        service: args.service(),
                        kind: args.selection.kind,
                        profile: args.profile(),
                        composer: args.targets.composer,
                        node: args.targets.node,
                        bun: args.targets.bun,
                        deno: args.targets.deno,
                        all: args.targets.all,
                        package_manager: args.selection.package_manager,
                        version_manager: args.selection.version_manager,
                        node_version: args.selection.node_version.as_deref(),
                        non_interactive: context.non_interactive(),
                        quiet: context.quiet(),
                        tty: args.selection.tty,
                        no_tty: args.selection.no_tty,
                        config_path: context.config_path(),
                        project_root: context.project_root(),
                    },
                )),
                TaskDepsCommands::Audit(args) => Some(handlers::handle_task_deps_workflow(
                    config,
                    handlers::HandleTaskDepsWorkflowOptions {
                        action: handlers::TaskDependencyAction::Audit,
                        service: args.service(),
                        kind: args.selection.kind,
                        profile: args.profile(),
                        composer: args.targets.composer,
                        node: args.targets.node,
                        bun: args.targets.bun,
                        deno: args.targets.deno,
                        all: args.targets.all,
                        package_manager: args.selection.package_manager,
                        version_manager: args.selection.version_manager,
                        node_version: args.selection.node_version.as_deref(),
                        non_interactive: context.non_interactive(),
                        quiet: context.quiet(),
                        tty: args.selection.tty,
                        no_tty: args.selection.no_tty,
                        config_path: context.config_path(),
                        project_root: context.project_root(),
                    },
                )),
                TaskDepsCommands::Normalize(args) => Some(handlers::handle_task_deps_workflow(
                    config,
                    handlers::HandleTaskDepsWorkflowOptions {
                        action: handlers::TaskDependencyAction::Normalize,
                        service: args.service(),
                        kind: args.selection.kind,
                        profile: args.profile(),
                        composer: args.targets.composer,
                        node: args.targets.node,
                        bun: args.targets.bun,
                        deno: args.targets.deno,
                        all: args.targets.all,
                        package_manager: args.selection.package_manager,
                        version_manager: args.selection.version_manager,
                        node_version: args.selection.node_version.as_deref(),
                        non_interactive: context.non_interactive(),
                        quiet: context.quiet(),
                        tty: args.selection.tty,
                        no_tty: args.selection.no_tty,
                        config_path: context.config_path(),
                        project_root: context.project_root(),
                    },
                )),
                TaskDepsCommands::Install(args) => Some(handlers::handle_task_deps_workflow(
                    config,
                    handlers::HandleTaskDepsWorkflowOptions {
                        action: handlers::TaskDependencyAction::Install,
                        service: args.service(),
                        kind: args.selection.kind,
                        profile: args.profile(),
                        composer: args.targets.composer,
                        node: args.targets.node,
                        bun: args.targets.bun,
                        deno: args.targets.deno,
                        all: args.targets.all,
                        package_manager: args.selection.package_manager,
                        version_manager: args.selection.version_manager,
                        node_version: args.selection.node_version.as_deref(),
                        non_interactive: context.non_interactive(),
                        quiet: context.quiet(),
                        tty: args.selection.tty,
                        no_tty: args.selection.no_tty,
                        config_path: context.config_path(),
                        project_root: context.project_root(),
                    },
                )),
            },
        },
        Commands::AppCreate(args) => Some(handlers::handle_app_create(
            config,
            handlers::HandleAppCreateOptions {
                service: args.service(),
                no_migrate: args.no_migrate,
                seed: args.seed,
                no_storage_link: args.no_storage_link,
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
    fn wrapper_dispatches_exec() {
        assert!(dispatch_result(&["berth", "exec"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_artisan() {
        assert!(dispatch_result(&["berth", "artisan"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_composer() {
        assert!(dispatch_result(&["berth", "composer"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_php_tools() {
        assert!(dispatch_result(&["berth", "phpstan"]).is_some());
        assert!(dispatch_result(&["berth", "ecs"]).is_some());
        assert!(dispatch_result(&["berth", "php-cs-fixer"]).is_some());
        assert!(dispatch_result(&["berth", "psalm"]).is_some());
        assert!(dispatch_result(&["berth", "pint"]).is_some());
        assert!(dispatch_result(&["berth", "pest"]).is_some());
        assert!(dispatch_result(&["berth", "phpunit"]).is_some());
        assert!(dispatch_result(&["berth", "rector"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_node() {
        assert!(dispatch_result(&["berth", "node"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_bun() {
        assert!(dispatch_result(&["berth", "bun"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_deno() {
        assert!(dispatch_result(&["berth", "deno"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_app_create() {
        assert!(dispatch_result(&["berth", "app-create"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_task() {
        assert!(dispatch_result(&["berth", "task", "deps", "bump", "--composer"]).is_some());
    }

    #[test]
    fn wrapper_dispatches_none_for_other_commands() {
        assert!(dispatch_result(&["berth", "status"]).is_none());
    }
}
