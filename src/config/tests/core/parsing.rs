use super::*;
#[test]
fn parse_app_service_section() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "db"
            kind = "database"
            driver = "mysql"
            image = "mysql:8.0"
            host = "127.0.0.1"
            port = 3306
            database = "app"
            username = "root"
            password = "secret"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.berth"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");

    assert_eq!(config.service.len(), 2);
    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service present");
    assert_eq!(app.name, "web");
    assert_eq!(app.domain.as_deref(), Some("donkey.berth"));
}

#[test]
fn parse_app_only_config() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.berth"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    assert_eq!(config.service.len(), 1);
    assert_eq!(config.service[0].kind, Kind::App);
}

#[test]
fn parse_app_domains_list() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domains = ["primary.berth", "alt.test", "alt.org"]
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    let app = &config.service[0];

    assert_eq!(app.domain, None);
    assert_eq!(
        app.domains.as_ref().expect("domains configured"),
        &vec![
            "primary.berth".to_owned(),
            "alt.test".to_owned(),
            "alt.org".to_owned()
        ]
    );
    assert_eq!(app.primary_domain(), Some("primary.berth"));
}

#[test]
fn parse_app_javascript_toolchain_section() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.berth"

            [service.javascript]
            runtime = "node"
            version_manager = "fnm"
            version = "22"
            package_manager = "pnpm"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    let app = &config.service[0];
    let javascript = app
        .javascript
        .as_ref()
        .expect("javascript toolchain configured");

    assert_eq!(javascript.version.as_deref(), Some("22"));
    assert_eq!(
        javascript.runtime,
        Some(crate::javascript::JavaScriptRuntime::Node)
    );
    assert_eq!(
        javascript.version_manager,
        Some(crate::javascript::VersionManager::Fnm)
    );
    assert_eq!(
        javascript.package_manager,
        Some(crate::javascript::PackageManager::Pnpm)
    );
}

#[test]
fn parse_app_deno_toolchain_section() {
    let toml = r#"
            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.berth"

            [service.javascript]
            runtime = "deno"
            version = "2.2.3"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    let javascript = config.service[0]
        .javascript
        .as_ref()
        .expect("deno runtime configured");

    assert_eq!(
        javascript.runtime,
        Some(crate::javascript::JavaScriptRuntime::Deno)
    );
    assert_eq!(javascript.version.as_deref(), Some("2.2.3"));
    assert!(javascript.package_manager.is_none());
}

#[test]
fn parse_service_restart_policy_override() {
    let toml = r#"
            [[service]]
            name = "db"
            kind = "database"
            driver = "mysql"
            image = "mysql:8.4"
            host = "127.0.0.1"
            port = 3306
            restart = "no"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");

    assert_eq!(config.service[0].restart, Some(RestartPolicy::No));
}

#[test]
fn parse_app_bun_toolchain_section() {
    let toml = r#"
            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.berth"

            [service.javascript]
            runtime = "bun"
            version = "1.2.5"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    let javascript = config.service[0]
        .javascript
        .as_ref()
        .expect("bun runtime configured");

    assert_eq!(
        javascript.runtime,
        Some(crate::javascript::JavaScriptRuntime::Bun)
    );
    assert_eq!(javascript.version.as_deref(), Some("1.2.5"));
    assert!(javascript.package_manager.is_none());
}
