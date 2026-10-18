use super::*;

#[test]
fn preset_aliases_cover_all_primary_backends() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "mongodb"
            name = "mongo"

            [[service]]
            preset = "postgres"
            name = "pg"

            [[service]]
            preset = "pgsql"
            name = "pgsql"

            [[service]]
            preset = "sqlserver"
            name = "sqlserver"

            [[service]]
            preset = "mssql"
            name = "mssql"

            [[service]]
            preset = "valkey"
            name = "cache"

            [[service]]
            preset = "dragonfly"
            name = "dragonfly"

            [[service]]
            preset = "memcached"
            name = "mem"

            [[service]]
            preset = "rustfs"
            name = "storage"

            [[service]]
            preset = "garage"
            name = "garage"

            [[service]]
            preset = "localstack"
            name = "localstack"

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

            [[service]]
            preset = "frankenphp"
            name = "web"

            [[service]]
            preset = "reverb"
            name = "ws"

            [[service]]
            preset = "mariadb"
            name = "maria"

            [[service]]
            preset = "rabbitmq"
            name = "queue"

            [[service]]
            preset = "soketi"
            name = "socket"

            [[service]]
            preset = "scheduler"
            name = "cron"

            [[service]]
            preset = "queue"
            name = "worker"

            [[service]]
            preset = "selenium"
            name = "browser"

            [[service]]
            preset = "mailpit"
            name = "mailpit"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let mongo = config
        .service
        .iter()
        .find(|svc| svc.name == "mongo")
        .expect("mongo service");
    assert_eq!(mongo.driver, Driver::Mongodb);
    assert_eq!(mongo.image, "mongo:8");

    let pg = config
        .service
        .iter()
        .find(|svc| svc.name == "pg")
        .expect("pg service");
    assert_eq!(pg.driver, Driver::Postgres);

    let pgsql = config
        .service
        .iter()
        .find(|svc| svc.name == "pgsql")
        .expect("pgsql service");
    assert_eq!(pgsql.driver, Driver::Postgres);

    let sqlserver = config
        .service
        .iter()
        .find(|svc| svc.name == "sqlserver")
        .expect("sqlserver service");
    assert_eq!(sqlserver.kind, Kind::Database);
    assert_eq!(
        sqlserver.image,
        "mcr.microsoft.com/mssql/server:2022-latest"
    );

    let mssql = config
        .service
        .iter()
        .find(|svc| svc.name == "mssql")
        .expect("mssql service");
    assert_eq!(mssql.kind, Kind::Database);
    assert_eq!(mssql.image, "mcr.microsoft.com/mssql/server:2022-latest");

    let cache = config
        .service
        .iter()
        .find(|svc| svc.name == "cache")
        .expect("cache service");
    assert_eq!(cache.driver, Driver::Valkey);

    let dragonfly = config
        .service
        .iter()
        .find(|svc| svc.name == "dragonfly")
        .expect("dragonfly service");
    assert_eq!(dragonfly.kind, Kind::Cache);
    assert_eq!(
        dragonfly.image,
        "docker.dragonflydb.io/dragonflydb/dragonfly:v1.39.0"
    );

    let mem = config
        .service
        .iter()
        .find(|svc| svc.name == "mem")
        .expect("mem service");
    assert_eq!(mem.driver, Driver::Memcached);
    assert_eq!(mem.image, "memcached:1.6-alpine");

    let storage = config
        .service
        .iter()
        .find(|svc| svc.name == "storage")
        .expect("storage service");
    assert_eq!(storage.driver, Driver::Rustfs);

    let garage = config
        .service
        .iter()
        .find(|svc| svc.name == "garage")
        .expect("garage service");
    assert_eq!(garage.driver, Driver::Garage);
    assert_eq!(garage.image, "dxflrs/garage:v2.1.0");

    let localstack = config
        .service
        .iter()
        .find(|svc| svc.name == "localstack")
        .expect("localstack service");
    assert_eq!(localstack.kind, Kind::ObjectStore);
    assert_eq!(localstack.image, "localstack/localstack:4.12.0");

    let opensearch = config
        .service
        .iter()
        .find(|svc| svc.name == "search-opensearch")
        .expect("opensearch service");
    assert_eq!(opensearch.driver, Driver::Opensearch);

    let elasticsearch = config
        .service
        .iter()
        .find(|svc| svc.name == "search-elasticsearch")
        .expect("elasticsearch service");
    assert_eq!(elasticsearch.driver, Driver::Elasticsearch);

    let meili = config
        .service
        .iter()
        .find(|svc| svc.name == "search-meili")
        .expect("meili service");
    assert_eq!(meili.driver, Driver::Meilisearch);

    let typesense = config
        .service
        .iter()
        .find(|svc| svc.name == "search-typesense")
        .expect("typesense service");
    assert_eq!(typesense.driver, Driver::Typesense);

    let web = config
        .service
        .iter()
        .find(|svc| svc.name == "web")
        .expect("web service");
    assert_eq!(web.driver, Driver::Frankenphp);

    let ws = config
        .service
        .iter()
        .find(|svc| svc.name == "ws")
        .expect("ws service");
    assert_eq!(ws.driver, Driver::Reverb);

    let maria = config
        .service
        .iter()
        .find(|svc| svc.name == "maria")
        .expect("maria service");
    assert_eq!(maria.driver, Driver::Mysql);
    assert_eq!(maria.image, "mariadb:11");

    let queue = config
        .service
        .iter()
        .find(|svc| svc.name == "queue")
        .expect("queue service");
    assert_eq!(queue.driver, Driver::Rabbitmq);
    assert_eq!(queue.image, "rabbitmq:3-management");

    let socket = config
        .service
        .iter()
        .find(|svc| svc.name == "socket")
        .expect("socket service");
    assert_eq!(socket.driver, Driver::Soketi);
    assert_eq!(
        socket.image,
        "quay.io/soketi/soketi:5d188786beaf683aca2115a6247dcdc15c29ac77-16-debian"
    );

    let cron = config
        .service
        .iter()
        .find(|svc| svc.name == "cron")
        .expect("cron service");
    assert_eq!(cron.driver, Driver::Scheduler);
    assert_eq!(cron.image, "dunglas/frankenphp:php8.5");

    let worker = config
        .service
        .iter()
        .find(|svc| svc.name == "worker")
        .expect("worker service");
    assert_eq!(worker.driver, Driver::Frankenphp);
    assert_eq!(worker.image, "dunglas/frankenphp:php8.5");

    let browser = config
        .service
        .iter()
        .find(|svc| svc.name == "browser")
        .expect("browser service");
    assert_eq!(browser.driver, Driver::Dusk);
    assert_eq!(
        browser.image,
        "selenium/standalone-chromium:4.43.0-20260404"
    );

    let mailpit = config
        .service
        .iter()
        .find(|svc| svc.name == "mailpit")
        .expect("mailpit service");
    assert_eq!(mailpit.driver, Driver::Mailhog);
    assert_eq!(mailpit.image, "axllent/mailpit:v1.30.0");
}

#[test]
fn minio_preset_is_refused_with_its_replacements() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "minio"
            name = "storage"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let error = expansion::expand_raw_config(raw)
        .expect_err("minio is no longer a preset")
        .to_string();

    assert!(error.contains("preset 'minio' was removed"), "{error}");
    assert!(error.contains("'rustfs' or 'garage'"), "{error}");
}
