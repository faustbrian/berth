use super::*;

#[test]
fn resolve_swarm_root_handles_relative_and_absolute_paths() {
    let workspace = PathBuf::from("/tmp/workspace");
    let relative = resolve_swarm_root(&workspace, &PathBuf::from("api"));
    assert_eq!(relative, PathBuf::from("/tmp/workspace/api"));

    let absolute_input = PathBuf::from("/tmp/elsewhere");
    let absolute = resolve_swarm_root(&workspace, &absolute_input);
    assert_eq!(absolute, absolute_input);
}

#[test]
fn swarm_child_args_forwards_global_flags_and_project_root() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned(), "--profile".to_owned(), "infra".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        true,
        true,
        true,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--quiet",
            "--no-color",
            "--dry-run",
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--profile",
            "infra",
            "--publish-all",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_forwards_runtime_env_flag() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        Some("test"),
    );

    assert_eq!(
        args,
        vec![
            "--env",
            "test",
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--publish-all",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_preserves_publish_all_for_up() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned(), "--publish-all".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--publish-all",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_does_not_add_publish_all_for_non_up_commands() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["down".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec!["--project-root", "/tmp/ws/api", "down", "--no-deps"]
    );
}

#[test]
fn swarm_child_args_adds_env_output_for_up_when_requested() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        true,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--publish-all",
            "--env-output",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_keeps_publish_all_in_repro_mode() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--publish-all",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_adds_stable_port_strategy_flags() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["up".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Stable,
        Some("workspace-seed"),
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "up",
            "--publish-all",
            "--port-strategy",
            "stable",
            "--port-seed",
            "workspace-seed",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_adds_publish_all_for_recreate() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    let command = vec!["recreate".to_owned()];

    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );

    assert_eq!(
        args,
        vec![
            "--project-root",
            "/tmp/ws/api",
            "recreate",
            "--publish-all",
            "--no-deps",
        ]
    );
}

#[test]
fn swarm_child_args_repeat_the_requested_engine_and_repro() {
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    crate::followup::record(crate::followup::Invocation {
        engine: Some(crate::config::ContainerEngine::Podman),
        repro: true,
        instance: Some("agent-2".to_owned()),
        ..crate::followup::Invocation::default()
    });
    let args = swarm_child_args(
        &target,
        &["up".to_owned()],
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );
    crate::followup::record(crate::followup::Invocation::default());
    assert_eq!(
        &args[..5],
        [
            "--engine",
            "podman",
            "--repro",
            "--project-root",
            "/tmp/ws/api"
        ]
    );
    // Every target has its own instance.
    assert!(!args.iter().any(|arg| arg == "--instance"), "{args:?}");
}

#[test]
fn swarm_child_args_repeat_settings_and_parse_as_berth_reads_them() {
    use clap::Parser;
    let target = ResolvedSwarmTarget {
        name: "api".to_owned(),
        root: PathBuf::from("/tmp/ws/api"),
    };
    crate::followup::record(crate::followup::Invocation {
        engine: Some(crate::config::ContainerEngine::Podman),
        repro: true,
        settings: vec![
            ("--non-interactive", None),
            ("--docker-max-heavy-ops", Some("2".to_owned())),
            ("--docker-max-build-ops", Some("3".to_owned())),
            ("--docker-retry-budget", Some("4".to_owned())),
            ("--test-runtime-pool-size", Some("5".to_owned())),
        ],
        ..crate::followup::Invocation::default()
    });
    let command = vec![
        "start".to_owned(),
        "--engine".to_owned(),
        "docker".to_owned(),
    ];
    let args = swarm_child_args(
        &target,
        &command,
        PortStrategyArg::Random,
        None,
        false,
        false,
        false,
        false,
        None,
    );
    crate::followup::record(crate::followup::Invocation::default());
    let cli = Cli::try_parse_from(std::iter::once("berth".to_owned()).chain(args))
        .expect("child args parse");
    // A swarm `start` child must not open browsers in a non-interactive run.
    assert!(cli.non_interactive && cli.repro);
    assert_eq!(cli.docker_max_heavy_ops, Some(2));
    assert_eq!(cli.docker_max_build_ops, Some(3));
    assert_eq!(cli.docker_retry_budget, Some(4));
    assert_eq!(cli.test_runtime_pool_size, Some(5));
    // The swarm command's own value wins over the run's.
    assert_eq!(cli.engine, Some(crate::config::ContainerEngine::Docker));
}
