use super::super::*;

#[test]
fn preset_config_expands_defaults_and_assigns_ports() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "mysql"
            name = "acme"

            [[service]]
            preset = "mysql"
            name = "billing"

            [[service]]
            preset = "redis"

            [[service]]
            preset = "rustfs"

            [[service]]
            preset = "laravel"
            domain = "acme-api.berth"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    assert_eq!(config.service.len(), 5);
    let acme = config
        .service
        .iter()
        .find(|svc| svc.name == "acme")
        .expect("acme service");
    assert_eq!(acme.driver, Driver::Mysql);
    assert_eq!(acme.port, 33060);

    let billing = config
        .service
        .iter()
        .find(|svc| svc.name == "billing")
        .expect("billing service");
    assert_eq!(billing.driver, Driver::Mysql);
    assert_eq!(billing.port, 33061);

    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service");
    assert_eq!(app.name, "app");
    assert_eq!(app.port, 33065);
    assert_eq!(app.domain.as_deref(), Some("acme-api.berth"));
    assert_eq!(
        app.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}

#[test]
fn laravel_preset_forces_local_app_env() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "laravel"
            env = { APP_ENV = "production" }
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service");

    assert_eq!(
        app.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}

#[test]
fn laravel_preset_includes_sockets_extension() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "laravel"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service");

    let extensions = app
        .php_extensions
        .as_ref()
        .expect("laravel preset php extensions");
    assert!(extensions.iter().any(|ext| ext == "sockets"));
}

#[test]
fn reverb_preset_sets_runtime_command_and_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "reverb"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let reverb = config.service.first().expect("reverb service");

    assert_eq!(reverb.name, "reverb");
    assert_eq!(reverb.driver, Driver::Reverb);
    assert_eq!(reverb.port, 33068);
    assert_eq!(reverb.container_port, Some(8080));
    assert_eq!(
        reverb.command,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "reverb:start".to_owned(),
            "--host=0.0.0.0".to_owned(),
            "--port=8080".to_owned(),
        ])
    );
}

#[test]
fn horizon_preset_sets_runtime_command_and_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "horizon"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let horizon = config.service.first().expect("horizon service");

    assert_eq!(horizon.name, "horizon");
    assert_eq!(horizon.driver, Driver::Horizon);
    assert_eq!(horizon.port, 33069);
    assert_eq!(
        horizon.command,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "horizon".to_owned(),
        ])
    );
    assert_eq!(
        horizon.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
    assert_eq!(
        horizon
            .env
            .as_ref()
            .and_then(|env| env.get("QUEUE_CONNECTION")),
        Some(&"redis".to_owned())
    );
    let extensions = horizon
        .php_extensions
        .as_ref()
        .expect("horizon preset php extensions");
    assert!(extensions.iter().any(|ext| ext == "redis"));
}

#[test]
fn queue_worker_preset_sets_runtime_command_and_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "queue-worker"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let worker = config.service.first().expect("queue worker service");

    assert_eq!(worker.name, "queue-worker");
    assert_eq!(worker.driver, Driver::Frankenphp);
    assert_eq!(
        worker.command,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "queue:work".to_owned(),
            "--verbose".to_owned(),
            "--tries=3".to_owned(),
            "--timeout=90".to_owned(),
        ])
    );
    assert_eq!(
        worker.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}

#[test]
fn scheduler_preset_sets_runtime_command_and_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "scheduler"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let scheduler = config.service.first().expect("scheduler service");

    assert_eq!(scheduler.name, "scheduler");
    assert_eq!(scheduler.driver, Driver::Scheduler);
    assert_eq!(scheduler.port, 33071);
    assert_eq!(
        scheduler.command,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "schedule:work".to_owned(),
        ])
    );
    assert_eq!(
        scheduler.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}

#[test]
fn dusk_preset_sets_selenium_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "dusk"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let dusk = config.service.first().expect("dusk service");

    assert_eq!(dusk.name, "dusk");
    assert_eq!(dusk.driver, Driver::Dusk);
    assert_eq!(dusk.port, 33070);
    assert_eq!(dusk.container_port, Some(4444));
    assert_eq!(dusk.image, "selenium/standalone-chromium:latest");
}

#[test]
fn selenium_preset_sets_standalone_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "selenium"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let selenium = config.service.first().expect("selenium service");

    assert_eq!(selenium.name, "selenium");
    assert_eq!(selenium.driver, Driver::Dusk);
    assert_eq!(selenium.port, 33070);
    assert_eq!(selenium.container_port, Some(4444));
    assert_eq!(selenium.image, "selenium/standalone-chromium:latest");
}

#[test]
fn mailpit_preset_sets_mail_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "mailpit"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let mailpit = config.service.first().expect("mailpit service");

    assert_eq!(mailpit.name, "mailpit");
    assert_eq!(mailpit.driver, Driver::Mailhog);
    assert_eq!(mailpit.port, 33067);
    assert_eq!(mailpit.container_port, Some(8025));
    assert_eq!(mailpit.smtp_port, Some(1025));
    assert_eq!(mailpit.image, "axllent/mailpit:latest");
}

#[test]
fn search_presets_set_laravel_scout_env_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "opensearch"
            name = "search-opensearch"

            [[service]]
            preset = "elasticsearch"
            name = "search-elasticsearch"

            [[service]]
            preset = "meilisearch"
            name = "search-meili"

            [[service]]
            preset = "typesense"
            name = "search-typesense"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let opensearch = config
        .service
        .iter()
        .find(|svc| svc.name == "search-opensearch")
        .expect("opensearch service");
    assert_eq!(
        opensearch
            .env
            .as_ref()
            .and_then(|env| env.get("SCOUT_DRIVER")),
        Some(&"opensearch".to_owned())
    );
    assert_eq!(
        opensearch
            .env
            .as_ref()
            .and_then(|env| env.get("discovery.type")),
        Some(&"single-node".to_owned())
    );
    assert_eq!(
        opensearch
            .env
            .as_ref()
            .and_then(|env| env.get("DISABLE_SECURITY_PLUGIN")),
        Some(&"true".to_owned())
    );
    assert_eq!(opensearch.port, 9200);

    let elasticsearch = config
        .service
        .iter()
        .find(|svc| svc.name == "search-elasticsearch")
        .expect("elasticsearch service");
    assert_eq!(
        elasticsearch
            .env
            .as_ref()
            .and_then(|env| env.get("SCOUT_DRIVER")),
        Some(&"elasticsearch".to_owned())
    );
    assert_eq!(
        elasticsearch
            .env
            .as_ref()
            .and_then(|env| env.get("discovery.type")),
        Some(&"single-node".to_owned())
    );
    assert_eq!(
        elasticsearch
            .env
            .as_ref()
            .and_then(|env| env.get("xpack.security.enabled")),
        Some(&"false".to_owned())
    );
    assert_eq!(elasticsearch.port, 9201);

    let meili = config
        .service
        .iter()
        .find(|svc| svc.name == "search-meili")
        .expect("meili service");
    assert_eq!(
        meili.env.as_ref().and_then(|env| env.get("SCOUT_DRIVER")),
        Some(&"meilisearch".to_owned())
    );
    assert_eq!(
        meili
            .env
            .as_ref()
            .and_then(|env| env.get("MEILISEARCH_KEY")),
        Some(&"masterKey".to_owned())
    );

    let typesense = config
        .service
        .iter()
        .find(|svc| svc.name == "search-typesense")
        .expect("typesense service");
    assert_eq!(
        typesense
            .env
            .as_ref()
            .and_then(|env| env.get("SCOUT_DRIVER")),
        Some(&"typesense".to_owned())
    );
    assert_eq!(
        typesense
            .env
            .as_ref()
            .and_then(|env| env.get("TYPESENSE_API_KEY")),
        Some(&"xyz".to_owned())
    );
}

#[test]
fn new_infra_presets_expand_expected_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "dragonfly"

            [[service]]
            preset = "sqlserver"
            name = "sql-db"

            [[service]]
            preset = "localstack"

            [[service]]
            preset = "garage"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let dragonfly = config
        .service
        .iter()
        .find(|svc| svc.driver == Driver::Dragonfly)
        .expect("dragonfly service");
    assert_eq!(dragonfly.port, 6380);

    let sqlserver = config
        .service
        .iter()
        .find(|svc| svc.name == "sql-db")
        .expect("sqlserver service");
    assert_eq!(sqlserver.driver, Driver::Sqlserver);
    assert_eq!(sqlserver.port, 14330);
    assert_eq!(sqlserver.username.as_deref(), Some("sa"));
    assert_eq!(
        sqlserver.password.as_deref(),
        Some("BerthSqlServerPassw0rd!")
    );

    let localstack = config
        .service
        .iter()
        .find(|svc| svc.driver == Driver::Localstack)
        .expect("localstack service");
    assert_eq!(localstack.port, 4566);
    assert_eq!(localstack.bucket.as_deref(), Some("media"));
    assert_eq!(localstack.access_key.as_deref(), Some("test"));
    assert_eq!(localstack.secret_key.as_deref(), Some("test"));

    let garage = config
        .service
        .iter()
        .find(|svc| svc.driver == Driver::Garage)
        .expect("garage service");
    assert_eq!(garage.port, 3900);
    assert_eq!(garage.bucket.as_deref(), Some("media"));
    assert_eq!(garage.access_key.as_deref(), Some("minio"));
    assert_eq!(garage.secret_key.as_deref(), Some("miniosecret"));
}

#[test]
fn rabbitmq_preset_sets_broker_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "rabbitmq"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let queue = config.service.first().expect("rabbitmq service");

    assert_eq!(queue.name, "rabbitmq");
    assert_eq!(queue.driver, Driver::Rabbitmq);
    assert_eq!(queue.port, 5672);
    assert_eq!(queue.container_port, Some(5672));
    assert_eq!(queue.username.as_deref(), Some("guest"));
    assert_eq!(queue.password.as_deref(), Some("guest"));
}

#[test]
fn soketi_preset_sets_broadcast_defaults() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "soketi"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let soketi = config.service.first().expect("soketi service");

    assert_eq!(soketi.name, "soketi");
    assert_eq!(soketi.driver, Driver::Soketi);
    assert_eq!(soketi.port, 6001);
    assert_eq!(soketi.container_port, Some(6001));
    assert_eq!(
        soketi
            .env
            .as_ref()
            .and_then(|env| env.get("SOKETI_DEFAULT_APP_ID")),
        Some(&"app-id".to_owned())
    );
}
