use super::*;
use std::path::PathBuf;

#[test]
fn swarm_cli_defaults_to_dependency_expansion() {
    let cli = Cli::try_parse_from(["berth", "swarm", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(!args.no_deps),
        _ => panic!("expected swarm command"),
    }

    let cli =
        Cli::try_parse_from(["berth", "swarm", "--no-deps", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.no_deps),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_force_flag() {
    let cli =
        Cli::try_parse_from(["berth", "swarm", "--force", "down"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.force),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn up_and_down_cli_parse_project_dependency_flags() {
    let up_default = Cli::try_parse_from(["berth", "up"]).expect("parse up");
    match up_default.command {
        Commands::Up(args) => assert!(!args.no_deps),
        _ => panic!("expected up command"),
    }

    let up = Cli::try_parse_from(["berth", "up", "--no-deps"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.no_deps),
        _ => panic!("expected up command"),
    }

    let down = Cli::try_parse_from(["berth", "down", "--force"]).expect("parse down");
    match down.command {
        Commands::Down(args) => {
            assert!(!args.no_deps);
            assert!(args.force);
        }
        _ => panic!("expected down command"),
    }
}

#[test]
fn up_cli_parses_publish_all_flag() {
    let up = Cli::try_parse_from(["berth", "up", "--publish-all"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.publish_all),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_no_wait_and_no_publish_all_flags() {
    let up =
        Cli::try_parse_from(["berth", "up", "--no-wait", "--no-publish-all"]).expect("parse up");
    match up.command {
        Commands::Up(args) => {
            assert!(args.no_wait);
            assert!(args.no_publish_all);
        }
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_port_strategy_and_seed_flags() {
    let up = Cli::try_parse_from([
        "berth",
        "up",
        "--port-strategy",
        "stable",
        "--port-seed",
        "workspace",
    ])
    .expect("parse up");
    match up.command {
        Commands::Up(args) => {
            assert!(matches!(args.port_strategy, PortStrategyArg::Stable));
            assert_eq!(args.port_seed.as_deref(), Some("workspace"));
        }
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_env_output_flag() {
    let up = Cli::try_parse_from(["berth", "up", "--env-output"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.env_output),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_seed_flag() {
    let up = Cli::try_parse_from(["berth", "up", "--seed"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.seed),
        _ => panic!("expected up command"),
    }
}

#[test]
fn swarm_cli_parses_port_strategy_and_seed_flags() {
    let cli = Cli::try_parse_from([
        "berth",
        "swarm",
        "--port-strategy",
        "stable",
        "--port-seed",
        "workspace",
        "up",
    ])
    .expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => {
            assert!(matches!(args.port_strategy, PortStrategyArg::Stable));
            assert_eq!(args.port_seed.as_deref(), Some("workspace"));
        }
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_env_output_flag() {
    let cli =
        Cli::try_parse_from(["berth", "swarm", "--env-output", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.env_output),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn removed_config_rewriting_flags_are_rejected() {
    // Berth never writes runtime ports back into the committed project file.
    assert!(Cli::try_parse_from(["berth", "env", "--sync", "--persist-runtime"]).is_err());
    assert!(Cli::try_parse_from(["berth", "up", "-P", "--save-ports"]).is_err());
    assert!(Cli::try_parse_from(["berth", "recreate", "-P", "--save-ports"]).is_err());
}

#[test]
fn env_cli_parses_generate_subcommand() {
    let cli = Cli::try_parse_from(["berth", "env", "generate", "--output", ".env.generated"])
        .expect("parse env generate");
    match cli.command {
        Commands::Env(args) => assert!(matches!(
            args.command,
            Some(crate::cli::args::EnvCommands::Generate { .. })
        )),
        _ => panic!("expected env command"),
    }
}

#[test]
fn cli_parses_global_runtime_env_flag() {
    let cli = Cli::try_parse_from(["berth", "--env", "test", "up"]).expect("parse global env");
    assert_eq!(cli.env.as_deref(), Some("test"));
}

#[test]
fn cli_parses_global_docker_policy_flags() {
    let cli = Cli::try_parse_from([
        "berth",
        "--docker-max-heavy-ops",
        "3",
        "--docker-max-build-ops",
        "1",
        "--docker-retry-budget",
        "5",
        "--test-runtime-pool-size",
        "4",
        "up",
    ])
    .expect("parse docker policy flags");

    assert_eq!(cli.docker_max_heavy_ops, Some(3));
    assert_eq!(cli.docker_max_build_ops, Some(1));
    assert_eq!(cli.docker_retry_budget, Some(5));
    assert_eq!(cli.test_runtime_pool_size, Some(4));
}

#[test]
fn cli_parses_global_repro_flag() {
    let cli = Cli::try_parse_from(["berth", "--repro", "up"]).expect("parse global repro");
    assert!(cli.repro);
}

#[test]
fn cli_parses_global_container_engine_flag() {
    let cli =
        Cli::try_parse_from(["berth", "--engine", "podman", "up"]).expect("parse global engine");
    assert_eq!(cli.engine, Some(crate::config::ContainerEngine::Podman));
}

#[test]
fn cli_parses_about_command() {
    let cli = Cli::try_parse_from(["berth", "about"]).expect("parse about");
    assert!(matches!(cli.command, Commands::About(_)));
}

#[test]
fn cli_parses_ls_command() {
    let ls = Cli::try_parse_from(["berth", "ls"]).expect("parse ls");
    assert!(matches!(ls.command, Commands::Ls(_)));
}

#[test]
fn cli_parses_status_alias_as_ps_command() {
    let status = Cli::try_parse_from(["berth", "status"]).expect("parse status alias");
    assert!(matches!(status.command, Commands::Ps(_)));
}

#[test]
fn cli_parses_apply_and_update_commands() {
    let apply = Cli::try_parse_from(["berth", "apply"]).expect("parse apply");
    assert!(matches!(apply.command, Commands::Apply(_)));

    let update = Cli::try_parse_from(["berth", "update"]).expect("parse update");
    assert!(matches!(update.command, Commands::Update(_)));
}

#[test]
fn cli_parses_task_deps_bump_targets() {
    let composer = Cli::try_parse_from(["berth", "task", "deps", "bump", "--composer"])
        .expect("parse task deps bump composer");
    match composer.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Bump(args),
            }) => {
                assert!(args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(!args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected bump task command"),
        },
        _ => panic!("expected task command"),
    }

    let node = Cli::try_parse_from(["berth", "task", "deps", "bump", "--node"])
        .expect("parse task deps bump node");
    match node.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Bump(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(args.targets.node);
                assert!(!args.targets.bun);
                assert!(!args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected bump task command"),
        },
        _ => panic!("expected task command"),
    }

    let bun =
        Cli::try_parse_from(["berth", "task", "deps", "bump", "--bun"]).expect("parse task deps");
    match bun.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Bump(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(args.targets.bun);
                assert!(!args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected bump task command"),
        },
        _ => panic!("expected task command"),
    }

    let deno = Cli::try_parse_from(["berth", "task", "deps", "bump", "--deno"])
        .expect("parse task deps bump deno");
    match deno.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Bump(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected bump task command"),
        },
        _ => panic!("expected task command"),
    }

    let all =
        Cli::try_parse_from(["berth", "task", "deps", "bump", "--all"]).expect("parse task deps");
    match all.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Bump(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(!args.targets.deno);
                assert!(args.targets.all);
            }
            _ => panic!("expected bump task command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_rejects_task_deps_bump_without_target_flag() {
    let result = Cli::try_parse_from(["berth", "task", "deps", "bump"]);
    assert!(result.is_err());
}

#[test]
fn cli_parses_task_deps_audit_targets() {
    let composer = Cli::try_parse_from(["berth", "task", "deps", "audit", "--composer"])
        .expect("parse task deps audit composer");
    match composer.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Audit(args),
            }) => {
                assert!(args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(!args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected audit task command"),
        },
        _ => panic!("expected task command"),
    }

    let bun = Cli::try_parse_from(["berth", "task", "deps", "audit", "--bun"])
        .expect("parse task deps audit bun");
    match bun.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Audit(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(args.targets.bun);
                assert!(!args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected audit task command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_deps_normalize_targets() {
    let deno = Cli::try_parse_from(["berth", "task", "deps", "normalize", "--deno"])
        .expect("parse task deps normalize deno");
    match deno.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Normalize(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(args.targets.deno);
                assert!(!args.targets.all);
            }
            _ => panic!("expected normalize task command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_parses_task_deps_install_all_targets() {
    let all = Cli::try_parse_from(["berth", "task", "deps", "install", "--all"])
        .expect("parse task deps install all");
    match all.command {
        Commands::Task(args) => match args.command {
            crate::cli::args::TaskCommands::Deps(crate::cli::args::TaskDepsArgs {
                command: crate::cli::args::TaskDepsCommands::Install(args),
            }) => {
                assert!(!args.targets.composer);
                assert!(!args.targets.node);
                assert!(!args.targets.bun);
                assert!(!args.targets.deno);
                assert!(args.targets.all);
            }
            _ => panic!("expected install task command"),
        },
        _ => panic!("expected task command"),
    }
}

#[test]
fn cli_rejects_new_task_deps_workflows_without_target_flag() {
    assert!(Cli::try_parse_from(["berth", "task", "deps", "audit"]).is_err());
    assert!(Cli::try_parse_from(["berth", "task", "deps", "normalize"]).is_err());
    assert!(Cli::try_parse_from(["berth", "task", "deps", "install"]).is_err());
}

#[test]
fn cli_rejects_removed_connect_list_shell_commands() {
    assert!(Cli::try_parse_from(["berth", "connect"]).is_err());
    assert!(Cli::try_parse_from(["berth", "list"]).is_err());
    assert!(Cli::try_parse_from(["berth", "shell"]).is_err());
}

#[test]
fn config_cli_parses_migrate_subcommand() {
    let cli = Cli::try_parse_from(["berth", "config", "migrate"]).expect("parse config migrate");
    match cli.command {
        Commands::Config(args) => assert!(matches!(
            args.command,
            Some(crate::cli::args::ConfigCommands::Migrate)
        )),
        _ => panic!("expected config command"),
    }
}

#[test]
fn doctor_cli_parses_repro_flag() {
    let cli = Cli::try_parse_from(["berth", "doctor", "--repro"]).expect("parse doctor repro");
    match cli.command {
        Commands::Doctor(args) => assert!(args.repro),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn doctor_cli_parses_reachability_flag() {
    let cli = Cli::try_parse_from(["berth", "doctor", "--reachability"])
        .expect("parse doctor reachability");
    match cli.command {
        Commands::Doctor(args) => assert!(args.reachability),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn doctor_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["berth", "doctor", "--format", "json"])
        .expect("parse doctor json format");
    match cli.command {
        Commands::Doctor(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn health_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["berth", "health", "--format", "json"])
        .expect("parse health json format");
    match cli.command {
        Commands::Health(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected health command"),
    }
}

#[test]
fn about_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["berth", "about", "--format", "json"])
        .expect("parse about json format");
    match cli.command {
        Commands::About(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected about command"),
    }
}

#[test]
fn lifecycle_commands_parse_profile_flag() {
    let down = Cli::try_parse_from(["berth", "down", "--profile", "infra"]).expect("parse down");
    match down.command {
        Commands::Down(args) => assert_eq!(args.profile(), Some("infra")),
        _ => panic!("expected down command"),
    }

    let stop = Cli::try_parse_from(["berth", "stop", "--profile", "data"]).expect("parse stop");
    match stop.command {
        Commands::Stop(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected stop command"),
    }

    let rm = Cli::try_parse_from(["berth", "rm", "--profile", "app"]).expect("parse rm");
    match rm.command {
        Commands::Rm(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected rm command"),
    }

    let recreate =
        Cli::try_parse_from(["berth", "recreate", "--profile", "full"]).expect("parse recreate");
    match recreate.command {
        Commands::Recreate(args) => assert_eq!(args.profile(), Some("full")),
        _ => panic!("expected recreate command"),
    }

    let restart =
        Cli::try_parse_from(["berth", "restart", "--profile", "web"]).expect("parse restart");
    match restart.command {
        Commands::Restart(args) => assert_eq!(args.profile(), Some("web")),
        _ => panic!("expected restart command"),
    }
}

#[test]
fn down_stop_restart_parse_repeated_service_flags() {
    let down = Cli::try_parse_from(["berth", "down", "--service", "db", "--service", "cache"])
        .expect("parse down repeated service");
    match down.command {
        Commands::Down(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected down command"),
    }

    let stop = Cli::try_parse_from(["berth", "stop", "--service", "db", "--service", "cache"])
        .expect("parse stop repeated service");
    match stop.command {
        Commands::Stop(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected stop command"),
    }

    let restart =
        Cli::try_parse_from(["berth", "restart", "--service", "db", "--service", "cache"])
            .expect("parse restart repeated service");
    match restart.command {
        Commands::Restart(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected restart command"),
    }
}

#[test]
fn logs_cli_parses_since_until() {
    let cli = Cli::try_parse_from([
        "berth",
        "logs",
        "--since",
        "5m",
        "--until",
        "2026-02-20T10:00:00Z",
    ])
    .expect("parse logs since/until");
    match cli.command {
        Commands::Logs(args) => {
            assert_eq!(args.since.as_deref(), Some("5m"));
            assert_eq!(args.until.as_deref(), Some("2026-02-20T10:00:00Z"));
        }
        _ => panic!("expected logs command"),
    }
}

#[test]
fn cli_parses_non_interactive_global_flag() {
    let cli =
        Cli::try_parse_from(["berth", "--non-interactive", "up"]).expect("parse non interactive");
    assert!(cli.non_interactive);
}

#[test]
fn app_runtime_commands_parse_kind_and_profile_selectors() {
    let exec = Cli::try_parse_from(["berth", "exec", "--kind", "app", "--", "php", "-v"])
        .expect("parse exec kind selector");
    match exec.command {
        Commands::Exec(args) => {
            assert_eq!(args.kind, Some(crate::config::Kind::App));
            assert_eq!(args.profile(), None);
        }
        _ => panic!("expected exec command"),
    }

    let serve = Cli::try_parse_from(["berth", "serve", "--profile", "web"])
        .expect("parse serve profile selector");
    match serve.command {
        Commands::Serve(args) => assert_eq!(args.profile(), Some("web")),
        _ => panic!("expected serve command"),
    }

    let open = Cli::try_parse_from(["berth", "open", "--profile", "app", "--no-browser"])
        .expect("parse open profile selector");
    match open.command {
        Commands::Open(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected open command"),
    }
}

#[test]
fn ops_commands_parse_profile_and_repeated_service_selectors() {
    let pull = Cli::try_parse_from(["berth", "pull", "--profile", "infra"]).expect("parse pull");
    match pull.command {
        Commands::Pull(args) => assert_eq!(args.profile(), Some("infra")),
        _ => panic!("expected pull command"),
    }

    let relabel =
        Cli::try_parse_from(["berth", "relabel", "--service", "db", "--service", "cache"])
            .expect("parse relabel repeated service");
    match relabel.command {
        Commands::Relabel(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected relabel command"),
    }

    let health =
        Cli::try_parse_from(["berth", "health", "--profile", "data"]).expect("parse health");
    match health.command {
        Commands::Health(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected health command"),
    }

    let logs =
        Cli::try_parse_from(["berth", "logs", "--profile", "app"]).expect("parse logs profile");
    match logs.command {
        Commands::Logs(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected logs command"),
    }
}

#[test]
fn docker_ops_parse_profile_and_repeated_services() {
    let inspect = Cli::try_parse_from([
        "berth",
        "inspect",
        "--service",
        "db",
        "--service",
        "cache",
        "--profile",
        "infra",
    ]);
    assert!(inspect.is_err());

    let inspect =
        Cli::try_parse_from(["berth", "inspect", "--service", "db", "--service", "cache"])
            .expect("parse inspect repeated service");
    match inspect.command {
        Commands::Inspect(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected inspect command"),
    }

    let port = Cli::try_parse_from(["berth", "port", "--format", "json"]).expect("parse port");
    match port.command {
        Commands::Port(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected port command"),
    }

    let kill = Cli::try_parse_from(["berth", "kill", "--profile", "data"]).expect("parse kill");
    match kill.command {
        Commands::Kill(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected kill command"),
    }
}

#[test]
fn start_cli_parses_core_flags() {
    let cli = Cli::try_parse_from([
        "berth",
        "start",
        "--profile",
        "app",
        "--no-open",
        "--no-wait",
        "--parallel",
        "4",
    ])
    .expect("parse start");
    match cli.command {
        Commands::Start(args) => {
            assert_eq!(args.profile.as_deref(), Some("app"));
            assert!(args.no_open);
            assert!(args.no_wait);
            assert_eq!(args.parallel, 4);
        }
        _ => panic!("expected start command"),
    }
}

#[test]
fn recreate_cli_defaults_to_wait_and_allows_no_wait_override() {
    let recreate = Cli::try_parse_from(["berth", "recreate"]).expect("parse recreate");
    match recreate.command {
        Commands::Recreate(args) => {
            assert!(args.should_wait());
            assert!(!args.no_wait);
        }
        _ => panic!("expected recreate command"),
    }

    let recreate_no_wait =
        Cli::try_parse_from(["berth", "recreate", "--no-wait"]).expect("parse recreate no-wait");
    match recreate_no_wait.command {
        Commands::Recreate(args) => {
            assert!(!args.should_wait());
            assert!(args.no_wait);
        }
        _ => panic!("expected recreate command"),
    }
}

#[test]
fn app_commands_parse_service_flag() {
    let serve = Cli::try_parse_from(["berth", "serve", "--service", "app"]).expect("parse serve");
    match serve.command {
        Commands::Serve(args) => assert_eq!(args.service.as_deref(), Some("app")),
        _ => panic!("expected serve command"),
    }

    let artisan = Cli::try_parse_from(["berth", "artisan", "--service", "app", "--", "about"])
        .expect("parse artisan");
    match artisan.command {
        Commands::Artisan(args) => assert_eq!(args.service.as_deref(), Some("app")),
        _ => panic!("expected artisan command"),
    }
}

#[test]
fn app_artisan_cli_parses_browser_flag() {
    let artisan =
        Cli::try_parse_from(["berth", "artisan", "--browser", "test"]).expect("parse artisan");
    match artisan.command {
        Commands::Artisan(args) => {
            assert!(args.browser);
            assert_eq!(args.command, vec!["test".to_owned()]);
        }
        _ => panic!("expected artisan command"),
    }
}

#[test]
fn app_runtime_commands_default_tty_to_true() {
    let exec = Cli::try_parse_from(["berth", "exec", "--", "php", "-v"]).expect("parse exec");
    match exec.command {
        Commands::Exec(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected exec command"),
    }

    let artisan = Cli::try_parse_from(["berth", "artisan", "about"]).expect("parse artisan");
    match artisan.command {
        Commands::Artisan(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected artisan command"),
    }

    let composer =
        Cli::try_parse_from(["berth", "composer", "--", "install"]).expect("parse composer");
    match composer.command {
        Commands::Composer(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected composer command"),
    }

    let phpstan =
        Cli::try_parse_from(["berth", "phpstan", "--", "analyse"]).expect("parse phpstan");
    match phpstan.command {
        Commands::Phpstan(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected phpstan command"),
    }

    let ecs = Cli::try_parse_from(["berth", "ecs", "--", "check"]).expect("parse ecs");
    match ecs.command {
        Commands::Ecs(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected ecs command"),
    }

    let fixer =
        Cli::try_parse_from(["berth", "php-cs-fixer", "--", "fix"]).expect("parse php-cs-fixer");
    match fixer.command {
        Commands::PhpCsFixer(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected php-cs-fixer command"),
    }

    let psalm =
        Cli::try_parse_from(["berth", "psalm", "--", "--show-info=false"]).expect("parse psalm");
    match psalm.command {
        Commands::Psalm(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected psalm command"),
    }

    let pint = Cli::try_parse_from(["berth", "pint", "--", "--dirty"]).expect("parse pint");
    match pint.command {
        Commands::Pint(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected pint command"),
    }

    let pest =
        Cli::try_parse_from(["berth", "pest", "--", "--filter=Feature"]).expect("parse pest");
    match pest.command {
        Commands::Pest(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected pest command"),
    }

    let phpunit =
        Cli::try_parse_from(["berth", "phpunit", "--", "--testsuite=Unit"]).expect("parse phpunit");
    match phpunit.command {
        Commands::Phpunit(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected phpunit command"),
    }

    let rector = Cli::try_parse_from(["berth", "rector", "--", "process"]).expect("parse rector");
    match rector.command {
        Commands::Rector(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected rector command"),
    }

    let node = Cli::try_parse_from(["berth", "node", "--", "run", "dev"]).expect("parse node");
    match node.command {
        Commands::Node(args) => {
            assert!(args.tty);
            assert!(!args.no_tty);
        }
        _ => panic!("expected node command"),
    }
}

#[test]
fn node_cli_parses_package_manager_and_version_manager_flags() {
    let cli = Cli::try_parse_from([
        "berth",
        "node",
        "--package-manager",
        "pnpm",
        "--version-manager",
        "fnm",
        "--node-version",
        "22",
        "--",
        "run",
        "dev",
    ])
    .expect("parse node");

    match cli.command {
        Commands::Node(args) => {
            assert!(matches!(
                args.package_manager,
                Some(crate::cli::args::PackageManagerArg::Pnpm)
            ));
            assert!(matches!(
                args.version_manager,
                Some(crate::cli::args::VersionManagerArg::Fnm)
            ));
            assert_eq!(args.node_version.as_deref(), Some("22"));
            assert_eq!(args.command, vec!["run".to_owned(), "dev".to_owned()]);
        }
        _ => panic!("expected node command"),
    }
}

#[test]
fn deno_cli_parses_deno_version_flag() {
    let cli = Cli::try_parse_from([
        "berth",
        "deno",
        "--deno-version",
        "2.2.3",
        "--",
        "task",
        "dev",
    ])
    .expect("parse deno");

    match cli.command {
        Commands::Deno(args) => {
            assert_eq!(args.deno_version.as_deref(), Some("2.2.3"));
            assert_eq!(args.command, vec!["task".to_owned(), "dev".to_owned()]);
        }
        _ => panic!("expected deno command"),
    }
}

#[test]
fn bun_cli_parses_bun_version_flag() {
    let cli = Cli::try_parse_from(["berth", "bun", "--bun-version", "1.2.5", "--", "run", "dev"])
        .expect("parse bun");

    match cli.command {
        Commands::Bun(args) => {
            assert_eq!(args.bun_version.as_deref(), Some("1.2.5"));
            assert_eq!(args.command, vec!["run".to_owned(), "dev".to_owned()]);
        }
        _ => panic!("expected bun command"),
    }
}

#[test]
fn php_tool_commands_parse_arguments_like_composer() {
    let phpstan = Cli::try_parse_from(["berth", "phpstan", "--service", "app", "--", "analyse"])
        .expect("parse phpstan");
    match phpstan.command {
        Commands::Phpstan(args) => {
            assert_eq!(args.service.as_deref(), Some("app"));
            assert_eq!(args.command, vec!["analyse".to_owned()]);
        }
        _ => panic!("expected phpstan command"),
    }

    let ecs = Cli::try_parse_from(["berth", "ecs", "--no-tty", "--", "check"]).expect("parse ecs");
    match ecs.command {
        Commands::Ecs(args) => {
            assert!(args.tty);
            assert!(args.no_tty);
            assert_eq!(args.command, vec!["check".to_owned()]);
        }
        _ => panic!("expected ecs command"),
    }

    let fixer = Cli::try_parse_from(["berth", "php-cs-fixer", "--", "fix", "--dry-run"])
        .expect("parse php-cs-fixer");
    match fixer.command {
        Commands::PhpCsFixer(args) => {
            assert_eq!(args.command, vec!["fix".to_owned(), "--dry-run".to_owned()]);
        }
        _ => panic!("expected php-cs-fixer command"),
    }

    let psalm = Cli::try_parse_from(["berth", "psalm", "--", "--shepherd"]).expect("parse psalm");
    match psalm.command {
        Commands::Psalm(args) => {
            assert_eq!(args.command, vec!["--shepherd".to_owned()]);
        }
        _ => panic!("expected psalm command"),
    }

    let pint = Cli::try_parse_from(["berth", "pint", "--service", "app", "--", "--dirty"])
        .expect("parse pint");
    match pint.command {
        Commands::Pint(args) => {
            assert_eq!(args.service.as_deref(), Some("app"));
            assert_eq!(args.command, vec!["--dirty".to_owned()]);
        }
        _ => panic!("expected pint command"),
    }

    let pest =
        Cli::try_parse_from(["berth", "pest", "--no-tty", "--", "--parallel"]).expect("parse pest");
    match pest.command {
        Commands::Pest(args) => {
            assert!(args.tty);
            assert!(args.no_tty);
            assert_eq!(args.command, vec!["--parallel".to_owned()]);
        }
        _ => panic!("expected pest command"),
    }

    let phpunit =
        Cli::try_parse_from(["berth", "phpunit", "--", "--filter", "Unit"]).expect("parse phpunit");
    match phpunit.command {
        Commands::Phpunit(args) => {
            assert_eq!(args.command, vec!["--filter".to_owned(), "Unit".to_owned()]);
        }
        _ => panic!("expected phpunit command"),
    }

    let rector =
        Cli::try_parse_from(["berth", "rector", "--", "process", "src"]).expect("parse rector");
    match rector.command {
        Commands::Rector(args) => {
            assert_eq!(args.command, vec!["process".to_owned(), "src".to_owned()]);
        }
        _ => panic!("expected rector command"),
    }
}

#[test]
fn app_runtime_commands_allow_disabling_tty_with_no_tty() {
    let artisan = Cli::try_parse_from(["berth", "artisan", "--no-tty", "test"])
        .expect("parse artisan no-tty");
    match artisan.command {
        Commands::Artisan(args) => {
            assert!(args.tty);
            assert!(args.no_tty);
        }
        _ => panic!("expected artisan command"),
    }
}

#[test]
fn share_cli_parses_start_status_stop_subcommands() {
    let start = Cli::try_parse_from([
        "berth",
        "share",
        "start",
        "--service",
        "app",
        "--provider",
        "cloudflare",
        "--detached",
    ])
    .expect("parse share start");
    match start.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert_eq!(share_args.service.as_deref(), Some("app"));
                assert!(matches!(
                    share_args.provider,
                    Some(crate::cli::args::ShareProviderArg::Cloudflare)
                ));
                assert!(share_args.detached);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let status = Cli::try_parse_from(["berth", "share", "status", "--provider", "tailscale"])
        .expect("parse share status");
    match status.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Status(share_args) => assert!(matches!(
                share_args.provider,
                Some(crate::cli::args::ShareProviderArg::Tailscale)
            )),
            _ => panic!("expected share status subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let shorthand = Cli::try_parse_from(["berth", "share", "start", "--tailscale"])
        .expect("parse share start shorthand");
    match shorthand.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert!(share_args.provider.is_none());
                assert!(share_args.tailscale);
                assert!(!share_args.cloudflare);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let expose = Cli::try_parse_from(["berth", "share", "start", "--expose"])
        .expect("parse share start expose shorthand");
    match expose.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert!(share_args.provider.is_none());
                assert!(share_args.expose);
                assert!(!share_args.cloudflare);
                assert!(!share_args.tailscale);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let timeout =
        Cli::try_parse_from(["berth", "share", "start", "--tailscale", "--timeout", "45"])
            .expect("parse share start timeout");
    match timeout.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert_eq!(share_args.timeout, 45)
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let stop = Cli::try_parse_from(["berth", "share", "stop", "--all"]).expect("parse share stop");
    match stop.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Stop(share_args) => assert!(share_args.all),
            _ => panic!("expected share stop subcommand"),
        },
        _ => panic!("expected share command"),
    }
}

#[test]
fn lock_cli_parses_subcommands() {
    let images = Cli::try_parse_from(["berth", "lock", "images"]).expect("parse lock images");
    match images.command {
        Commands::Lock(args) => assert!(matches!(
            args.command,
            crate::cli::args::LockCommands::Images
        )),
        _ => panic!("expected lock command"),
    }

    let verify = Cli::try_parse_from(["berth", "lock", "verify"]).expect("parse lock verify");
    match verify.command {
        Commands::Lock(args) => assert!(matches!(
            args.command,
            crate::cli::args::LockCommands::Verify
        )),
        _ => panic!("expected lock command"),
    }

    let diff = Cli::try_parse_from(["berth", "lock", "diff"]).expect("parse lock diff");
    match diff.command {
        Commands::Lock(args) => {
            assert!(matches!(args.command, crate::cli::args::LockCommands::Diff))
        }
        _ => panic!("expected lock command"),
    }
}

#[test]
fn daemon_cli_parses_subcommands() {
    let start = Cli::try_parse_from(["berth", "daemon", "start", "--path", "/tmp/project"])
        .expect("parse daemon start");
    match start.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Start(start_args) => {
                assert_eq!(start_args.path, PathBuf::from("/tmp/project"));
            }
            _ => panic!("expected daemon start subcommand"),
        },
        _ => panic!("expected daemon command"),
    }

    let watch = Cli::try_parse_from([
        "berth",
        "daemon",
        "watch",
        "--dir",
        "/tmp/projects",
        "--dir",
        "/tmp/work",
        "--exclude-dir",
        "/tmp/archive",
        "--max-projects",
        "8",
        "--once",
        "--interval",
        "15",
    ])
    .expect("parse daemon watch");
    match watch.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Watch(watch_args) => {
                assert_eq!(
                    watch_args.dir,
                    vec![PathBuf::from("/tmp/projects"), PathBuf::from("/tmp/work")]
                );
                assert_eq!(
                    watch_args.policy.exclude_dir,
                    vec![PathBuf::from("/tmp/archive")]
                );
                assert_eq!(watch_args.policy.max_projects, Some(8));
                assert!(watch_args.once);
                assert_eq!(watch_args.interval, 15);
            }
            _ => panic!("expected daemon watch subcommand"),
        },
        _ => panic!("expected daemon command"),
    }

    let service_install = Cli::try_parse_from([
        "berth",
        "daemon",
        "service",
        "install",
        "--dir",
        "/tmp/projects",
        "--dir",
        "/tmp/work",
        "--exclude-dir",
        "/tmp/archive",
        "--max-projects",
        "3",
        "--interval",
        "45",
    ])
    .expect("parse daemon service install");
    match service_install.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Service(service_args) => match service_args.command {
                crate::cli::args::DaemonServiceCommands::Install(install_args) => {
                    assert_eq!(
                        install_args.dir,
                        vec![PathBuf::from("/tmp/projects"), PathBuf::from("/tmp/work")]
                    );
                    assert_eq!(
                        install_args.policy.exclude_dir,
                        vec![PathBuf::from("/tmp/archive")]
                    );
                    assert_eq!(install_args.policy.max_projects, Some(3));
                    assert_eq!(install_args.interval, 45);
                }
                _ => panic!("expected daemon service install subcommand"),
            },
            _ => panic!("expected daemon service subcommand"),
        },
        _ => panic!("expected daemon command"),
    }

    let status = Cli::try_parse_from(["berth", "daemon", "status", "--path", "/tmp/project"])
        .expect("parse daemon status");
    match status.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Status(status_args) => {
                assert_eq!(status_args.path, PathBuf::from("/tmp/project"));
            }
            _ => panic!("expected daemon status subcommand"),
        },
        _ => panic!("expected daemon command"),
    }

    let stop = Cli::try_parse_from(["berth", "daemon", "stop", "--path", "/tmp/project"])
        .expect("parse daemon stop");
    match stop.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Stop(stop_args) => {
                assert_eq!(stop_args.path, PathBuf::from("/tmp/project"));
            }
            _ => panic!("expected daemon stop subcommand"),
        },
        _ => panic!("expected daemon command"),
    }

    let logs = Cli::try_parse_from(["berth", "daemon", "logs", "--path", "/tmp/project"])
        .expect("parse daemon logs");
    match logs.command {
        Commands::Daemon(args) => match args.command {
            crate::cli::args::DaemonCommands::Logs(logs_args) => {
                assert_eq!(logs_args.path, PathBuf::from("/tmp/project"));
            }
            _ => panic!("expected daemon logs subcommand"),
        },
        _ => panic!("expected daemon command"),
    }
}
