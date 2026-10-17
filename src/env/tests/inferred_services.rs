use super::super::inferred_app_env;
use super::helpers::svc;
use crate::config::{Config, Driver, Kind};
use std::collections::HashMap;

#[test]
fn inferred_app_env_includes_core_laravel_service_vars() {
    let mut db = svc("db", Kind::Database, Driver::Mysql, 33060);
    db.database = Some("laravel".to_owned());
    db.username = Some("laravel".to_owned());
    db.password = Some("laravel".to_owned());

    let mut redis = svc("redis", Kind::Cache, Driver::Redis, 6380);
    redis.password = Some("secret".to_owned());

    let memcached = svc("memcached", Kind::Cache, Driver::Memcached, 11211);

    let mut s3 = svc("s3", Kind::ObjectStore, Driver::Rustfs, 9000);
    s3.bucket = Some("media".to_owned());
    s3.access_key = Some("minio".to_owned());
    s3.secret_key = Some("miniosecret".to_owned());
    s3.region = Some("us-east-1".to_owned());

    let gotenberg = svc("gotenberg", Kind::App, Driver::Gotenberg, 33066);
    let mut mailhog = svc("mailhog", Kind::App, Driver::Mailhog, 33067);
    mailhog.smtp_port = Some(34067);
    let reverb = svc("reverb", Kind::App, Driver::Reverb, 33068);
    let dusk = svc("dusk", Kind::App, Driver::Dusk, 33070);
    let rabbitmq = svc("rabbitmq", Kind::App, Driver::Rabbitmq, 5672);
    let soketi = svc("soketi", Kind::App, Driver::Soketi, 6001);
    let scheduler = svc("scheduler", Kind::App, Driver::Scheduler, 33071);

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![
            db, redis, memcached, s3, gotenberg, mailhog, reverb, dusk, rabbitmq, soketi, scheduler,
        ],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(
        vars.get("DB_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("DB_PORT"), Some(&"33060".to_owned()));
    assert_eq!(
        vars.get("BERTH_SQL_CLIENT_FLAVOR"),
        Some(&"mysql".to_owned())
    );
    assert_eq!(vars.get("QUEUE_CONNECTION"), Some(&"redis".to_owned()));
    assert_eq!(
        vars.get("REDIS_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(
        vars.get("REDIS_CACHE_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("REDIS_CACHE_PORT"), Some(&"6380".to_owned()));
    assert_eq!(vars.get("CACHE_STORE"), Some(&"redis".to_owned()));
    assert_eq!(
        vars.get("MEMCACHED_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("MEMCACHED_PORT"), Some(&"11211".to_owned()));
    assert_eq!(vars.get("AWS_BUCKET"), Some(&"media".to_owned()));
    assert_eq!(
        vars.get("AWS_URL"),
        Some(&"http://host.docker.internal:9000".to_owned())
    );
    assert_eq!(
        vars.get("GOTENBERG_BASE_URL"),
        Some(&"http://host.docker.internal:33066".to_owned())
    );
    assert_eq!(vars.get("MAIL_MAILER"), Some(&"smtp".to_owned()));
    assert_eq!(
        vars.get("MAIL_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("MAIL_PORT"), Some(&"34067".to_owned()));
    assert_eq!(vars.get("MAIL_ENCRYPTION"), Some(&"null".to_owned()));
    assert_eq!(vars.get("BROADCAST_CONNECTION"), Some(&"reverb".to_owned()));
    assert_eq!(
        vars.get("REVERB_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("REVERB_PORT"), Some(&"33068".to_owned()));
    assert_eq!(vars.get("REVERB_SCHEME"), Some(&"http".to_owned()));
    assert_eq!(
        vars.get("RABBITMQ_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("RABBITMQ_PORT"), Some(&"5672".to_owned()));
    assert_eq!(vars.get("RABBITMQ_USER"), Some(&"guest".to_owned()));
    assert_eq!(vars.get("RABBITMQ_PASSWORD"), Some(&"guest".to_owned()));
    assert_eq!(vars.get("PUSHER_APP_ID"), Some(&"app-id".to_owned()));
    assert_eq!(vars.get("PUSHER_APP_KEY"), Some(&"app-key".to_owned()));
    assert_eq!(
        vars.get("PUSHER_APP_SECRET"),
        Some(&"app-secret".to_owned())
    );
    assert_eq!(
        vars.get("PUSHER_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("PUSHER_PORT"), Some(&"6001".to_owned()));
    assert_eq!(vars.get("PUSHER_SCHEME"), Some(&"http".to_owned()));
    assert_eq!(vars.get("APP_ENV"), Some(&"local".to_owned()));
    assert_eq!(
        vars.get("DUSK_DRIVER_URL"),
        Some(&"http://host.docker.internal:33070/wd/hub".to_owned())
    );
}

#[test]
fn inferred_app_env_uses_mariadb_flavor_for_mariadb_only_database_images() {
    let mut db = svc("db", Kind::Database, Driver::Mysql, 33060);
    db.image = "mariadb:11".to_owned();

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![db],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(
        vars.get("BERTH_SQL_CLIENT_FLAVOR"),
        Some(&"mariadb".to_owned())
    );
}

#[test]
fn inferred_app_env_includes_opensearch_scout_vars() {
    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![svc("search", Kind::Search, Driver::Opensearch, 9200)],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("SCOUT_DRIVER"), Some(&"opensearch".to_owned()));
    assert_eq!(
        vars.get("OPENSEARCH_HOST"),
        Some(&"http://host.docker.internal:9200".to_owned())
    );
}

#[test]
fn inferred_app_env_includes_elasticsearch_scout_vars() {
    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![svc("search", Kind::Search, Driver::Elasticsearch, 9200)],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("SCOUT_DRIVER"), Some(&"elasticsearch".to_owned()));
    assert_eq!(
        vars.get("ELASTICSEARCH_HOST"),
        Some(&"http://host.docker.internal:9200".to_owned())
    );
}

#[test]
fn inferred_app_env_includes_meilisearch_scout_vars() {
    let mut meili = svc("search", Kind::Search, Driver::Meilisearch, 7700);
    meili.api_key = Some("masterKey".to_owned());

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![meili],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("SCOUT_DRIVER"), Some(&"meilisearch".to_owned()));
    assert_eq!(
        vars.get("MEILISEARCH_HOST"),
        Some(&"http://host.docker.internal:7700".to_owned())
    );
    assert_eq!(vars.get("MEILISEARCH_KEY"), Some(&"masterKey".to_owned()));
}

#[test]
fn inferred_app_env_includes_typesense_scout_vars() {
    let mut typesense = svc("search", Kind::Search, Driver::Typesense, 8108);
    typesense.api_key = Some("xyz".to_owned());

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![typesense],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("SCOUT_DRIVER"), Some(&"typesense".to_owned()));
    assert_eq!(
        vars.get("TYPESENSE_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("TYPESENSE_PORT"), Some(&"8108".to_owned()));
    assert_eq!(vars.get("TYPESENSE_PROTOCOL"), Some(&"http".to_owned()));
    assert_eq!(vars.get("TYPESENSE_API_KEY"), Some(&"xyz".to_owned()));
}

#[test]
fn inferred_app_env_includes_horizon_queue_defaults() {
    let horizon = svc("horizon", Kind::App, Driver::Horizon, 33069);

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![horizon],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("QUEUE_CONNECTION"), Some(&"redis".to_owned()));
}

#[test]
fn inferred_app_env_prefers_object_store_domain_for_aws_url() {
    let mut s3 = svc("s3", Kind::ObjectStore, Driver::Rustfs, 9000);
    s3.domain = Some("media.berth".to_owned());

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![s3],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("AWS_URL"), Some(&"https://media.berth".to_owned()));
    assert_eq!(
        vars.get("AWS_ENDPOINT"),
        Some(&"http://host.docker.internal:9000".to_owned())
    );
}

#[test]
fn inferred_app_env_includes_scheduler_defaults() {
    let scheduler = svc("scheduler", Kind::App, Driver::Scheduler, 33071);

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![scheduler],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("APP_ENV"), Some(&"local".to_owned()));
}

#[test]
fn inferred_app_env_uses_sqlsrv_connection_for_sqlserver() {
    let mut db = svc("db", Kind::Database, Driver::Sqlserver, 14330);
    db.database = Some("laravel".to_owned());
    db.username = Some("sa".to_owned());
    db.password = Some("BerthSqlServerPassw0rd!".to_owned());

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("app".to_owned()),
        domain_strategy: None,
        service: vec![db],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(vars.get("DB_CONNECTION"), Some(&"sqlsrv".to_owned()));
    assert_eq!(
        vars.get("DB_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("DB_PORT"), Some(&"14330".to_owned()));
}

#[test]
fn inferred_app_env_applies_service_env_mapping_without_overwriting_primary_db() {
    let mut primary = svc("shipit", Kind::Database, Driver::Mysql, 33060);
    primary.database = Some("shipit".to_owned());
    primary.username = Some("shipit_user".to_owned());
    primary.password = Some("shipit_pass".to_owned());

    let mut invoicing = svc("billing", Kind::Database, Driver::Mysql, 33061);
    invoicing.database = Some("billing".to_owned());
    invoicing.username = Some("billing_user".to_owned());
    invoicing.password = Some("billing_pass".to_owned());
    invoicing.env_mapping = Some(HashMap::from([
        ("DB_HOST".to_owned(), "DB_INVOICING_HOST".to_owned()),
        ("DB_PORT".to_owned(), "DB_INVOICING_PORT".to_owned()),
        ("DB_DATABASE".to_owned(), "DB_INVOICING_DATABASE".to_owned()),
        ("DB_USERNAME".to_owned(), "DB_INVOICING_USERNAME".to_owned()),
        ("DB_PASSWORD".to_owned(), "DB_INVOICING_PASSWORD".to_owned()),
    ]));

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: Some("shipit-api".to_owned()),
        domain_strategy: None,
        service: vec![primary, invoicing],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(
        vars.get("DB_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("DB_PORT"), Some(&"33060".to_owned()));
    assert_eq!(vars.get("DB_DATABASE"), Some(&"shipit".to_owned()));
    assert_eq!(vars.get("DB_USERNAME"), Some(&"shipit_user".to_owned()));
    assert_eq!(vars.get("DB_PASSWORD"), Some(&"shipit_pass".to_owned()));
    assert_eq!(
        vars.get("DB_INVOICING_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("DB_INVOICING_PORT"), Some(&"33061".to_owned()));
    assert_eq!(
        vars.get("DB_INVOICING_DATABASE"),
        Some(&"billing".to_owned())
    );
    assert_eq!(
        vars.get("DB_INVOICING_USERNAME"),
        Some(&"billing_user".to_owned())
    );
    assert_eq!(
        vars.get("DB_INVOICING_PASSWORD"),
        Some(&"billing_pass".to_owned())
    );
}
