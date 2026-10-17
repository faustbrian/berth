use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{convert_legacy_config, migrate_project_file, parse_project_file};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "berth-project-file-{label}-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    drop(std::fs::remove_dir_all(&dir));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn parse(content: &str) -> anyhow::Result<crate::config::RawConfig> {
    parse_project_file(content, Path::new("/tmp/.berth.yaml"))
}

#[test]
fn parses_services_map_and_names_services_by_key() -> anyhow::Result<()> {
    let raw = parse(
        "schema_version: 9\nproject: shipit-api\nservices:\n  db:\n    preset: mysql\n  app:\n    preset: laravel\n",
    )?;
    assert_eq!(raw.container_prefix.as_deref(), Some("shipit-api"));
    let names = raw
        .service
        .iter()
        .map(|service| service.name.clone().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["db", "app"],
        "services keep their declaration order"
    );
    Ok(())
}

#[test]
fn rejects_unknown_top_level_and_service_fields() {
    let top = parse("schema_version: 9\nproject: api\ncontainer_prefix: api\n")
        .expect_err("unknown top-level field must fail");
    assert!(format!("{top:#}").contains("container_prefix"), "{top:#}");

    let service = parse(
        "schema_version: 9\nproject: api\nservices:\n  db:\n    preset: mysql\n    flavour: x\n",
    )
    .expect_err("unknown service field must fail");
    assert!(format!("{service:#}").contains("flavour"), "{service:#}");
}

#[test]
fn rejects_duplicate_service_keys() {
    let error = parse("schema_version: 9\nproject: api\nservices:\n  db:\n    preset: mysql\n  db:\n    preset: postgres\n")
        .expect_err("duplicate service keys must fail");
    assert!(format!("{error:#}").contains("duplicate"), "{error:#}");
}

#[test]
fn rejects_duplicate_env_keys() {
    let error = parse("schema_version: 9\nproject: api\nservices:\n  app:\n    preset: laravel\n    env:\n      APP_DEBUG: \"true\"\n      APP_DEBUG: \"false\"\n")
        .expect_err("duplicate env keys must fail");
    let message = format!("{error:#}");
    assert!(
        message.contains("duplicate") && message.contains("APP_DEBUG"),
        "{message}"
    );
}

#[test]
fn rejects_wrong_schema_version_and_missing_project() {
    let version = parse("schema_version: 8\nproject: api\n").expect_err("schema 8 must fail");
    assert!(
        format!("{version:#}").contains("schema_version 8"),
        "{version:#}"
    );
    let missing = parse("schema_version: 9\n").expect_err("project is required");
    assert!(format!("{missing:#}").contains("project"), "{missing:#}");
}

#[test]
fn rejects_service_name_field_and_invalid_project_names() {
    let named = parse(
        "schema_version: 9\nproject: api\nservices:\n  db:\n    name: other\n    preset: mysql\n",
    )
    .expect_err("service name field must fail");
    assert!(format!("{named:#}").contains("sets `name`"), "{named:#}");

    for invalid in ["Api", "api_v2", "-api", "a-very-long-project-name-x1"] {
        let error = parse(&format!("schema_version: 9\nproject: {invalid}\n"))
            .expect_err("invalid project name must fail");
        assert!(format!("{error:#}").contains(invalid), "{error:#}");
    }
}

#[test]
fn converts_v7_toml_service_lists() -> anyhow::Result<()> {
    let dir = temp_dir("v7");
    let converted = convert_legacy_config(
        "schema_version = 1\ncontainer_prefix = \"shipit-api\"\ndomain_strategy = \"directory\"\n\n[[service]]\npreset = \"laravel\"\nname = \"app\"\n\n[[service]]\npreset = \"redis\"\n\n[[service.hook]]\nname = \"warm\"\nphase = \"post_up\"\n[service.hook.run]\ntype = \"exec\"\nargv = [\"true\"]\n",
        &dir.join(".stackctl.toml"),
    )?;
    let raw = parse(&converted.content)?;
    assert_eq!(raw.container_prefix.as_deref(), Some("shipit-api"));
    let redis = raw
        .service
        .iter()
        .find(|service| service.name.as_deref() == Some("redis"))
        .expect("preset name becomes the service key");
    assert_eq!(redis.hook.len(), 1, "hooks survive conversion");
    assert!(
        !converted.notes.iter().any(|note| note.contains("domain")),
        "the directory strategy maps exactly: {:?}",
        converted.notes
    );
    Ok(())
}

#[test]
fn converts_v8_yaml_and_rejects_unconvertible_fields() -> anyhow::Result<()> {
    let dir = temp_dir("v8");
    let converted = convert_legacy_config(
        "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: laravel\n    environment:\n      APP_DEBUG: \"true\"\n  db:\n    preset: postgres\n",
        &dir.join(".stackctl.yaml"),
    )?;
    let raw = parse(&converted.content)?;
    let app = raw
        .service
        .iter()
        .find(|service| service.name.as_deref() == Some("app"))
        .expect("app service");
    assert_eq!(
        app.env
            .as_ref()
            .and_then(|env| env.get("APP_DEBUG"))
            .map(String::as_str),
        Some("true")
    );

    let pinned = convert_legacy_config(
        "schema_version: 8\nproject: bill\nservices:\n  db:\n    preset: postgres\n  cache:\n    preset: valkey\n    version: \"9\"\n",
        &dir.join(".stackctl.yaml"),
    )?;
    assert!(
        pinned.content.contains("image: postgres:18"),
        "{}",
        pinned.content
    );
    assert!(
        pinned.content.contains("image: valkey/valkey:9"),
        "{}",
        pinned.content
    );

    let fixed = convert_legacy_config(
        "schema_version: 8\nproject: bill\nservices:\n  mail:\n    preset: mailpit\n    version: \"2\"\n",
        &dir.join(".stackctl.yaml"),
    )
    .expect_err("fixed catalog presets accept only their default version");
    assert!(format!("{fixed:#}").contains("mailpit"), "{fixed:#}");

    let workflows = convert_legacy_config(
        "schema_version: 8\nproject: bill\nworkflows: {}\n",
        &dir.join(".stackctl.yaml"),
    )
    .expect_err("workflows are not converted");
    assert!(
        format!("{workflows:#}").contains("workflows"),
        "{workflows:#}"
    );
    Ok(())
}

#[test]
fn derives_project_name_from_directory_when_prefix_is_missing() -> anyhow::Result<()> {
    let dir = temp_dir("derive").join("Shipit_API");
    std::fs::create_dir_all(&dir)?;
    let converted =
        convert_legacy_config("[[service]]\npreset = \"redis\"\n", &dir.join(".helm.toml"))?;
    assert!(
        converted.content.contains("project: shipit-api"),
        "{}",
        converted.content
    );
    assert!(converted.notes.iter().any(|note| note.contains("derived")));
    Ok(())
}

#[test]
fn migrate_writes_base_and_override_files_without_overwriting() -> anyhow::Result<()> {
    let dir = temp_dir("migrate");
    std::fs::write(dir.join(".stackctl.toml"), "container_prefix = \"api\"\n")?;
    std::fs::write(
        dir.join(".stackctl.testing.toml"),
        "container_prefix = \"api\"\n",
    )?;
    std::fs::write(dir.join(".stackctl.lock.toml"), "[images]\n")?;

    let migrated = migrate_project_file(&dir)?;
    let mut targets = migrated
        .iter()
        .map(|file| {
            file.target
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned()
        })
        .collect::<Vec<_>>();
    targets.sort();
    assert_eq!(targets, [".berth.testing.yaml", ".berth.yaml"]);

    let again = migrate_project_file(&dir).expect_err("existing targets are never overwritten");
    assert!(format!("{again:#}").contains("already exists"), "{again:#}");
    Ok(())
}

#[test]
fn migrate_writes_nothing_when_any_conversion_fails() -> anyhow::Result<()> {
    let dir = temp_dir("atomic");
    std::fs::write(dir.join(".stackctl.toml"), "container_prefix = \"api\"\n")?;
    std::fs::write(
        dir.join(".stackctl.testing.toml"),
        "container_prefix = \"API\"\n",
    )?;
    migrate_project_file(&dir).expect_err("invalid override fails migration");
    assert!(!dir.join(".berth.yaml").exists(), "no partial migration");
    Ok(())
}

#[test]
fn migration_preserves_declaration_order_for_ports_and_primary_database() -> anyhow::Result<()> {
    let dir = temp_dir("order");
    let path = dir.join(".berth.yaml");
    crate::config::project_file::write_test_config_file(
        &path,
        "container_prefix = \"api\"\nproject_type = \"project\"\n\n[[service]]\npreset = \"postgres\"\nname = \"primary\"\ndatabase = \"primary_db\"\n\n[[service]]\npreset = \"postgres\"\nname = \"analytics\"\ndatabase = \"analytics_db\"\n",
    )?;

    let config = crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
        Some(&path),
        None,
    ))?;
    let names = config
        .service
        .iter()
        .map(|service| service.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, ["primary", "analytics"]);
    assert!(
        config.service[0].port < config.service[1].port,
        "default ports follow declaration order"
    );
    let env = crate::env::inferred_app_env(&config);
    assert_eq!(
        env.get("DB_DATABASE").map(String::as_str),
        Some("primary_db")
    );
    Ok(())
}

#[test]
fn converter_rejects_random_domains_and_unknown_schema_versions() {
    let dir = temp_dir("reject");
    let random = convert_legacy_config(
        "container_prefix = \"api\"\ndomain_strategy = \"random\"\n",
        &dir.join(".stackctl.toml"),
    )
    .expect_err("random domains have no berth 9 equivalent");
    assert!(
        format!("{random:#}").contains("domain_strategy"),
        "{random:#}"
    );

    let version = convert_legacy_config(
        "schema_version = 2\ncontainer_prefix = \"api\"\n",
        &dir.join(".stackctl.toml"),
    )
    .expect_err("unknown schema versions are not guessed");
    assert!(
        format!("{version:#}").contains("schema_version 2"),
        "{version:#}"
    );
}

#[test]
fn converter_reports_generated_routes_when_no_strategy_was_set() -> anyhow::Result<()> {
    let dir = temp_dir("routes-note");
    let converted =
        convert_legacy_config("container_prefix = \"api\"\n", &dir.join(".stackctl.toml"))?;
    assert!(
        converted
            .notes
            .iter()
            .any(|note| note.contains("instance route")),
        "{:?}",
        converted.notes
    );
    Ok(())
}

#[test]
fn migrate_rejects_two_overrides_for_one_environment_and_writes_nothing() -> anyhow::Result<()> {
    let dir = temp_dir("duplicate-overrides");
    std::fs::write(dir.join(".stackctl.toml"), "container_prefix = \"api\"\n")?;
    std::fs::write(
        dir.join(".stackctl.testing.toml"),
        "container_prefix = \"api\"\n",
    )?;
    std::fs::write(
        dir.join(".helm.testing.toml"),
        "container_prefix = \"api\"\n",
    )?;

    let error = migrate_project_file(&dir).expect_err("ambiguous overrides must fail");
    assert!(
        format!("{error:#}").contains(".berth.testing.yaml"),
        "{error:#}"
    );
    let leftovers = std::fs::read_dir(&dir)?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".berth."))
        .count();
    assert_eq!(
        leftovers, 0,
        "a failed migration leaves no berth 9 files or staging files"
    );
    Ok(())
}

#[test]
fn rejects_unknown_fields_inside_hook_run_blocks() {
    let error = parse("schema_version: 9\nproject: api\nservices:\n  app:\n    preset: laravel\n    hook:\n      - name: warm\n        phase: post_up\n        run:\n          type: exec\n          argv: [\"true\"]\n          timout: 5\n")
        .expect_err("unknown hook run fields must fail");
    assert!(format!("{error:#}").contains("timout"), "{error:#}");
}

#[test]
fn legacy_yaml_scalars_keep_their_exact_text_through_migration() -> anyhow::Result<()> {
    let dir = temp_dir("yaml-text");
    let converted = convert_legacy_config(
        "container_prefix: api\nservice:\n  - preset: laravel\n    name: app\n    restart: no\n    port: 8080\n    env:\n      XDEBUG_MODE: off\n      APP_LOCALE: no\n      UMASK: 0022\n",
        &dir.join(".stackctl.yaml"),
    )?;
    let raw = parse(&converted.content)?;
    let app = raw
        .service
        .iter()
        .find(|service| service.name.as_deref() == Some("app"))
        .expect("app service");
    let env = app.env.as_ref().expect("env");
    assert_eq!(env.get("XDEBUG_MODE").map(String::as_str), Some("off"));
    assert_eq!(env.get("APP_LOCALE").map(String::as_str), Some("no"));
    assert_eq!(env.get("UMASK").map(String::as_str), Some("0022"));
    assert_eq!(app.restart, Some(crate::config::RestartPolicy::No));
    assert_eq!(app.port, Some(8080));
    Ok(())
}

#[test]
fn legacy_yaml_aliases_fail_with_an_instruction_to_inline_them() {
    let dir = temp_dir("yaml-alias");
    let error = convert_legacy_config(
        "container_prefix: api\nservice:\n  - preset: redis\n    env: &shared\n      A: \"1\"\n  - preset: mysql\n    env: *shared\n",
        &dir.join(".stackctl.yaml"),
    )
    .expect_err("aliases are not converted");
    assert!(
        format!("{error:#}").contains("write the value out in full"),
        "{error:#}"
    );
}

#[test]
fn migration_drops_container_names_once_and_warns_about_data_volumes() -> anyhow::Result<()> {
    let dir = temp_dir("container-names");
    let converted = convert_legacy_config(
        "container_prefix = \"api\"\n\n[[service]]\npreset = \"mysql\"\nname = \"db\"\ncontainer_name = \"legacy-db\"\n\n[[service]]\npreset = \"redis\"\ncontainer_name = \"legacy-redis\"\n",
        &dir.join(".stackctl.toml"),
    )?;
    assert!(
        !converted.content.contains("container_name"),
        "{}",
        converted.content
    );
    let container_notes = converted
        .notes
        .iter()
        .filter(|note| note.contains("container_name"))
        .count();
    assert_eq!(container_notes, 1, "{:?}", converted.notes);
    assert!(
        converted
            .notes
            .iter()
            .any(|note| note.contains("data volumes are not reused")),
        "{:?}",
        converted.notes
    );
    Ok(())
}

#[test]
fn migrate_drops_v7_domains_berth_9_cannot_serve_and_keeps_servable_ones() -> anyhow::Result<()> {
    let dir = temp_dir("v7-domains");
    let converted = convert_legacy_config(
        "schema_version = 1\ncontainer_prefix = \"shop\"\ndomain_strategy = \"directory\"\n\n[[service]]\npreset = \"laravel\"\nname = \"app\"\ndomain = \"acme.berth\"\ndomains = [\"shop.berth.localhost\", \"a.b.test\"]\n",
        &dir.join(".stackctl.toml"),
    )?;
    let raw = parse(&converted.content)?;
    let app = raw.service.first().expect("app");
    assert_eq!(app.domain, None, "{}", converted.content);
    assert_eq!(
        app.domains.as_deref(),
        Some(&["shop.berth.localhost".to_owned()][..])
    );
    assert!(
        converted
            .notes
            .iter()
            .any(|note| note.contains("acme.berth") && note.contains("a.b.test")),
        "{:?}",
        converted.notes
    );
    Ok(())
}

#[test]
fn a_migrated_v8_shared_database_loads_as_a_tenant() -> anyhow::Result<()> {
    let dir = temp_dir("v8-database");
    let converted = convert_legacy_config(
        "schema_version: 8\nproject: bill\nservices:\n  db:\n    preset: postgres\n    database: app\n",
        &dir.join(".stackctl.yaml"),
    )?;
    assert!(
        !converted.content.contains("database:"),
        "{}",
        converted.content
    );
    assert!(
        converted
            .notes
            .iter()
            .any(|note| note.contains("dropped `database`")),
        "{:?}",
        converted.notes
    );
    let path = dir.join(".berth.yaml");
    std::fs::write(&path, &converted.content)?;
    std::fs::write(dir.join("composer.json"), "{\"type\": \"project\"}")?;
    let config = crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
        Some(&path),
        None,
    ))?;
    assert!(config.service[0].runtime.shared.is_some());
    Ok(())
}

#[test]
fn a_migrated_7_0_database_with_container_settings_stays_dedicated() -> anyhow::Result<()> {
    let dir = temp_dir("v7-volumes");
    let converted = convert_legacy_config(
        "container_prefix = \"api\"\nproject_type = \"project\"\n\n[[service]]\npreset = \"mysql\"\nvolumes = [\"./dump:/dump\"]\n\n[[service]]\npreset = \"postgres\"\nname = \"pg\"\n",
        &dir.join(".stackctl.toml"),
    )?;
    let raw = parse(&converted.content)?;
    let isolation = |name: &str| {
        raw.service
            .iter()
            .find(|service| service.name.as_deref() == Some(name))
            .and_then(|service| service.isolation)
    };
    assert_eq!(
        isolation("db"),
        Some(crate::config::Isolation::Dedicated),
        "{}",
        converted.content
    );
    assert_eq!(
        isolation("pg"),
        None,
        "an unconfigured database becomes shared"
    );
    Ok(())
}

#[test]
fn privilege_fields_are_rejected_by_name() {
    // A worktree's project file must not grant container privileges (L003).
    for field in [
        "privileged: true",
        "cap_add: [SYS_ADMIN]",
        "devices: [/dev/kvm]",
        "network_mode: host",
        "pid: host",
        "security_opt: [seccomp=unconfined]",
        "user: root",
    ] {
        let name = field.split(':').next().expect("field name");
        let error = parse(&format!(
            "schema_version: 9\nproject: api\nservices:\n  app:\n    preset: laravel\n    {field}\n"
        ))
        .expect_err(field);
        assert!(format!("{error:#}").contains(name), "{field}: {error:#}");
    }
}

#[test]
fn engine_sockets_and_bad_env_keys_fail_at_parse() {
    let socket = parse(
        "schema_version: 9\nproject: api\nservices:\n  app:\n    preset: laravel\n    volumes: ['/var/run/docker.sock:/var/run/docker.sock']\n",
    )
    .expect_err("socket mount");
    assert!(
        format!("{socket:#}").contains("engine socket"),
        "{socket:#}"
    );
    let key = parse(
        "schema_version: 9\nproject: api\nservices:\n  search:\n    preset: opensearch\n    env:\n      discovery.type: single-node\n      A-B: x\n",
    )
    .expect_err("bad key");
    assert!(format!("{key:#}").contains("'A-B'"), "{key:#}");
}

#[test]
fn service_names_must_be_valid_as_written() {
    // Committed names are never repaired; the input is echoed (L007).
    let long = "a".repeat(64);
    for name in ["Bill", "a_b", "-db", "db-", long.as_str()] {
        let error = parse(&format!(
            "schema_version: 9\nproject: api\nservices:\n  {name}:\n    preset: mysql\n"
        ))
        .expect_err(name);
        assert!(
            format!("{error:#}").contains(&format!("'{name}'")),
            "{name}: {error:#}"
        );
    }
    parse(&format!(
        "schema_version: 9\nproject: api\nservices:\n  {}:\n    preset: mysql\n",
        "a".repeat(63)
    ))
    .expect("63 bytes is a valid label");
}
