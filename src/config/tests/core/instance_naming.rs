use super::*;

const PROJECT: &str = "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  db:\n    preset: mysql\n  app:\n    preset: laravel\n";

fn worktree(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("create worktree");
    std::fs::write(root.join(".berth.yaml"), PROJECT).expect("write project file");
    root
}

fn base(label: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join(format!(
        "berth-instance-naming-{label}-{}",
        std::process::id()
    ));
    drop(std::fs::remove_dir_all(&base));
    std::fs::create_dir_all(&base).expect("create base");
    base
}

fn load(root: &std::path::Path, instance: Option<&str>) -> Config {
    load_config_with(LoadConfigPathOptions::new(None, Some(root)).with_instance(instance))
        .expect("load project")
}

#[test]
fn two_worktrees_of_one_project_get_distinct_container_names() {
    let base = base("worktrees");
    let first = load(&worktree(&base, "agent-1"), None);
    let second = load(&worktree(&base, "agent-2"), None);

    let first_db = first
        .service
        .iter()
        .find(|service| service.name == "db")
        .expect("db");
    let second_db = second
        .service
        .iter()
        .find(|service| service.name == "db")
        .expect("db");
    let first_name = first_db.container_name().expect("name");
    let second_name = second_db.container_name().expect("name");

    assert_ne!(first_name, second_name);
    assert!(
        first_name.starts_with("berth_shipit-api_agent-1-"),
        "{first_name}"
    );
    assert!(first_name.ends_with("_db"), "{first_name}");
    assert_eq!(first_db.runtime.project.as_deref(), Some("shipit-api"));
    assert_ne!(first_db.runtime.instance, second_db.runtime.instance);
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn the_same_checkout_loads_the_same_instance_from_any_subdirectory() {
    let base = base("subdir");
    let root = worktree(&base, "checkout");
    std::fs::create_dir_all(root.join("app/Http")).expect("subdir");

    let from_root = load(&root, None);
    let from_subdir = load(&root.join("app/Http"), None);
    assert_eq!(
        from_root.service[0].runtime.instance,
        from_subdir.service[0].runtime.instance
    );
    assert_eq!(
        from_root.service[0].container_name().ok(),
        from_subdir.service[0].container_name().ok()
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn an_explicit_instance_overrides_the_derived_one() {
    let base = base("explicit");
    let config = load(&worktree(&base, "checkout"), Some("ci-7"));
    assert_eq!(
        config.service[0].container_name().ok().as_deref(),
        Some(format!("berth_shipit-api_ci-7_{}", config.service[0].name).as_str())
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn explicit_container_names_are_rejected() {
    let base = base("container-name");
    let root = base.join("checkout");
    std::fs::create_dir_all(&root).expect("create");
    std::fs::write(
        root.join(".berth.yaml"),
        "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  db:\n    preset: mysql\n    container_name: shared-db\n",
    )
    .expect("write");
    let error = load_config_with(LoadConfigPathOptions::new(None, Some(&root)))
        .expect_err("container_name must be rejected");
    assert!(format!("{error:#}").contains("container_name"), "{error:#}");
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn a_requested_instance_survives_reloads_of_the_same_project_only() {
    let base = base("reload");
    let main = worktree(&base, "checkout");
    let dependency = worktree(&base, "dependency");

    let first = load(&main, Some("ci-7"));
    let reloaded = load(&main, None);
    let other_project = load(&dependency, None);

    assert_eq!(first.service[0].runtime.instance.as_deref(), Some("ci-7"));
    assert_eq!(
        reloaded.service[0].runtime.instance.as_deref(),
        Some("ci-7")
    );
    assert_ne!(
        other_project.service[0].runtime.instance.as_deref(),
        Some("ci-7")
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn app_mounts_stay_host_paths_while_service_volumes_are_namespaced() {
    let base = base("mounts");
    let root = base.join("checkout");
    std::fs::create_dir_all(&root).expect("create");
    std::fs::write(
        root.join(".berth.yaml"),
        "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    volumes: [\".:/app\", \"docker/php.ini:/usr/local/etc/php/conf.d/zz.ini\", \"storage:/app/storage\"]\n  db:\n    preset: mysql\n    volumes: [\"dbdata:/var/lib/mysql\", \"./dump:/dump\"]\n",
    )
    .expect("write");
    let config = load(&root, Some("x-1"));
    let volumes = |name: &str| {
        config
            .service
            .iter()
            .find(|service| service.name == name)
            .and_then(|service| service.volumes.clone())
            .unwrap_or_default()
    };

    assert_eq!(
        volumes("app"),
        [
            ".:/app",
            "docker/php.ini:/usr/local/etc/php/conf.d/zz.ini",
            "storage:/app/storage"
        ]
    );
    assert_eq!(
        volumes("db"),
        ["berth_api_x-1_dbdata:/var/lib/mysql", "./dump:/dump"]
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn service_names_must_be_lowercase_dns_labels() {
    let base = base("service-names");
    let root = base.join("checkout");
    std::fs::create_dir_all(&root).expect("create");
    std::fs::write(
        root.join(".berth.yaml"),
        "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  queue_worker:\n    preset: redis\n",
    )
    .expect("write");
    let error = load_config_with(LoadConfigPathOptions::new(None, Some(&root)))
        .expect_err("underscores are reserved as name separators");
    assert!(format!("{error:#}").contains("queue_worker"), "{error:#}");
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn runtime_environments_get_their_own_network() {
    let base = base("env-network");
    let root = worktree(&base, "checkout");
    let mut config = load(&root, Some("ci-7"));
    assert_eq!(
        config.service[0].runtime.network.as_deref(),
        Some("berth_shipit-api_ci-7")
    );
    apply_runtime_env(&mut config, "testing").expect("apply testing env");
    assert_eq!(
        config.service[0].runtime.network.as_deref(),
        Some("berth_shipit-api_ci-7-testing")
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn runtime_environments_have_no_gateway_routes_even_with_long_test_pool_names() {
    let base = base("env-routes");
    let root = worktree(&base, "checkout");
    let mut config = load(&root, Some("abcdefghijklmnop-1a2b3c"));
    let routed: Vec<_> = config
        .service
        .iter()
        .filter(|service| service.resolved_domain.is_some())
        .map(|service| service.name.clone())
        .collect();
    apply_runtime_env(&mut config, "testing-pool-0123456789abcdef-12")
        .expect("a pooled test runtime never fails on route length");
    for service in &config.service {
        assert_eq!(service.resolved_domain, None, "{}", service.name);
        assert!(
            !crate::gateway::has_gateway_routes(service),
            "{}",
            service.name
        );
    }
    assert_eq!(routed, ["app"], "the default environment is routed");
    std::fs::remove_dir_all(&base).expect("cleanup");
}
