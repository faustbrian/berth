use super::*;

const PROJECT: &str = "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  db:\n    preset: mysql\n    isolation: dedicated\n  app:\n    preset: laravel\n";

fn worktree(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("create worktree");
    std::fs::write(root.join(".berth.yaml"), PROJECT).expect("write project file");
    root
}

fn base(label: &str) -> std::path::PathBuf {
    let base = crate::test_support::temp_root().join(format!(
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
        "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  db:\n    preset: mysql\n    isolation: dedicated\n    container_name: shared-db\n",
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
        "schema_version: 9\nproject: api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    volumes: [\".:/app\", \"docker/php.ini:/usr/local/etc/php/conf.d/zz.ini\", \"storage:/app/storage\"]\n  db:\n    preset: mysql\n    isolation: dedicated\n    volumes: [\"dbdata:/var/lib/mysql\", \"./dump:/dump\"]\n",
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

const ROUTED_PROJECT: &str = "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      ee: APP_EE_URL\n      fi: APP_FI_URL\n      se: APP_SE_URL\n";

fn routed_worktree(base: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("create worktree");
    std::fs::write(root.join(".berth.yaml"), contents).expect("write project file");
    root
}

fn app_route_hosts(config: &Config) -> Vec<String> {
    let app = config
        .service
        .iter()
        .find(|service| service.name == "app")
        .expect("app");
    crate::gateway::route_hosts(app)
}

#[test]
fn routes_give_every_instance_its_own_extra_hosts_after_its_primary_route() {
    let base = base("extra-routes");
    let root = routed_worktree(&base, "checkout", ROUTED_PROJECT);
    let first = load(&root, Some("ci-7"));
    let second = load(&root, Some("ci-8"));

    assert_eq!(
        app_route_hosts(&first),
        [
            "ci-7-shipit-api.berth.localhost",
            "ci-7-shipit-api-ee.berth.localhost",
            "ci-7-shipit-api-fi.berth.localhost",
            "ci-7-shipit-api-se.berth.localhost",
        ]
    );
    let second_hosts = app_route_hosts(&second);
    assert!(
        app_route_hosts(&first)
            .iter()
            .all(|host| !second_hosts.contains(host)),
        "{second_hosts:?}"
    );
    std::fs::create_dir_all(root.join("app")).expect("subdir");
    assert_eq!(
        app_route_hosts(&load(&root.join("app"), Some("ci-7"))),
        app_route_hosts(&first)
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn routes_are_not_served_outside_the_default_runtime_environment() {
    let base = base("extra-routes-env");
    let root = routed_worktree(&base, "checkout", ROUTED_PROJECT);
    for env in ["staging", "testing-pool-0123456789abcdef-12"] {
        let mut config = load(&root, Some("ci-7"));
        apply_runtime_env(&mut config, env).expect("apply env");
        assert!(app_route_hosts(&config).is_empty(), "{env}");
    }
    let mut local = load(&root, Some("ci-7"));
    apply_runtime_env(&mut local, "local").expect("apply local");
    assert_eq!(app_route_hosts(&local).len(), 4);
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn invalid_or_colliding_routes_fail_at_load_naming_the_route() {
    let base = base("extra-routes-invalid");
    let app = |routes: &str| {
        format!(
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n{routes}"
        )
    };
    let cases = [
        ("upper", app("      EE: APP_EE_URL\n"), "EE"),
        ("underscore", app("      e_e: APP_EE_URL\n"), "e_e"),
        ("dot", app("      ee.fi: APP_EE_URL\n"), "ee.fi"),
        ("leading", app("      -ee: APP_EE_URL\n"), "-ee"),
        ("trailing", app("      ee-: APP_EE_URL\n"), "ee-"),
        ("empty", app("      \"\": APP_EE_URL\n"), "app"),
        ("space", app("      e e: APP_EE_URL\n"), "e e"),
        ("duplicate", app("      ee: APP_EE_URL\n      ee: APP_FI_URL\n"), "ee"),
        ("same-var", app("      ee: APP_EE_URL\n      fi: APP_EE_URL\n"), "APP_EE_URL"),
        ("app-url", app("      ee: APP_URL\n"), "APP_URL"),
        ("asset-url", app("      ee: ASSET_URL\n"), "ASSET_URL"),
        (
            "no-routes",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes: {}\n".to_owned(),
            "`routes` is empty",
        ),
        ("app-key", app("      ee: APP_KEY\n"), "APP_KEY"),
        (
            "backend-var",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  s3:\n    preset: rustfs\n    isolation: dedicated\n  app:\n    preset: laravel\n    routes:\n      cdn: AWS_URL\n".to_owned(),
            "AWS_URL",
        ),
        (
            "localhost-tls",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    localhost_tls: true\n    routes:\n      ee: APP_EE_URL\n".to_owned(),
            "localhost_tls",
        ),
        (
            "var-in-two-services",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      ee: APP_EE_URL\n  admin:\n    preset: laravel\n    routes:\n      ee: APP_EE_URL\n".to_owned(),
            "APP_EE_URL",
        ),
        ("bad-var", app("      ee: APP-EE\n"), "APP-EE"),
        (
            "explicit-env",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    env:\n      APP_EE_URL: https://ee.example\n    routes:\n      ee: APP_EE_URL\n".to_owned(),
            "APP_EE_URL",
        ),
        (
            "service-collision",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      mail: APP_MAIL_URL\n  mail:\n    preset: mailpit\n".to_owned(),
            "ci-7-shipit-api-mail.berth.localhost",
        ),
        (
            "domain-collision",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    domain: ci-7-shipit-api-ee.berth.localhost\n    routes:\n      ee: APP_EE_URL\n".to_owned(),
            "ci-7-shipit-api-ee.berth.localhost",
        ),
        (
            "not-routable",
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n  cache:\n    preset: valkey\n    isolation: dedicated\n    routes:\n      ee: APP_EE_URL\n".to_owned(),
            "cache",
        ),
    ];
    for (name, contents, expected) in cases {
        let root = routed_worktree(&base, name, &contents);
        let error = load_config_with(
            LoadConfigPathOptions::new(None, Some(&root)).with_instance(Some("ci-7")),
        )
        .expect_err(name);
        let message = format!("{error:#}");
        assert!(message.contains(expected), "{name}: {message}");
    }
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn a_no_label_is_a_route_like_any_other() {
    let base = base("extra-routes-no");
    let root = routed_worktree(
        &base,
        "checkout",
        "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      no: APP_NO_URL\n",
    );
    assert_eq!(
        app_route_hosts(&load(&root, Some("ci-7")))[1],
        "ci-7-shipit-api-no.berth.localhost"
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn a_route_label_may_fill_but_not_exceed_the_dns_label_limit() {
    let base = base("extra-routes-length");
    // `ci-7-shipit-api-` is 16 bytes, leaving 47 for the route label.
    let fits = "a".repeat(47);
    let too_long = "a".repeat(48);
    let project = |label: &str| {
        format!(
            "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  app:\n    preset: laravel\n    routes:\n      {label}: APP_LONG_URL\n"
        )
    };
    let fits_root = routed_worktree(&base, "fits", &project(&fits));
    assert_eq!(
        app_route_hosts(&load(&fits_root, Some("ci-7")))[1],
        format!("ci-7-shipit-api-{fits}.berth.localhost")
    );
    let long_root = routed_worktree(&base, "long", &project(&too_long));
    let error = load_config_with(
        LoadConfigPathOptions::new(None, Some(&long_root)).with_instance(Some("ci-7")),
    )
    .expect_err("64-byte label");
    assert!(format!("{error:#}").contains("63"), "{error:#}");
    std::fs::remove_dir_all(&base).expect("cleanup");
}

#[test]
fn a_depends_on_typo_fails_at_load_naming_the_service_and_what_exists() {
    let base = base("depends-on");
    let root = routed_worktree(
        &base,
        "checkout",
        "schema_version: 9\nproject: shipit-api\nproject_type: project\nservices:\n  db:\n    preset: postgres\n  app:\n    preset: laravel\n    depends_on: [postgress]\n",
    );
    let error = load_config_with(LoadConfigPathOptions::new(None, Some(&root)))
        .expect_err("unknown dependency");
    let message = format!("{error:#}");
    assert!(
        message.contains("service 'app' lists 'postgress' in `depends_on`")
            && message.contains("services: db, app"),
        "{message}"
    );
    std::fs::remove_dir_all(&base).expect("cleanup");
}
