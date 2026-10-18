use super::super::images::{
    derived_image_tag, normalize_php_extensions, render_derived_dockerfile,
    should_inject_frankenphp_server_name,
};
use super::super::sql_client_flavor::SqlClientFlavor;
use super::helpers::app_service;
use crate::javascript::JavaScriptRuntime;
use crate::javascript::VersionManager;
use std::collections::HashMap;

#[test]
fn derived_image_tags_depend_on_content_only() {
    // Every instance needing the same image shares its tag (L029).
    let tag = derived_image_tag("dockerfile-fnv1a64-v1:00ff");
    assert!(tag.starts_with("berth/serve-"), "{tag}");
    assert_eq!(tag, derived_image_tag("dockerfile-fnv1a64-v1:00ff"));
    assert_ne!(tag, derived_image_tag("dockerfile-fnv1a64-v1:0100"));
}

#[test]
fn extension_dockerfile_contains_install_step() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &["pdo_mysql".to_owned(), "intl".to_owned()],
        false,
        JavaScriptRuntime::Node,
        VersionManager::System,
        None,
        SqlClientFlavor::Mysql,
        None,
    );
    assert!(rendered.contains("FROM dunglas/frankenphp:php8.5"));
    assert!(rendered.contains("RUN install-php-extensions pdo_mysql intl"));
    assert!(rendered.contains("/usr/local/bin/mysqldump"));
}

#[test]
fn derived_dockerfile_can_include_js_tooling() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Node,
        VersionManager::System,
        Some("22"),
        SqlClientFlavor::Mysql,
        None,
    );
    assert!(rendered.contains("npm install -g pnpm yarn"));
    assert!(!rendered.contains("corepack"));
    assert!(rendered.contains("getcomposer.org/installer"));
    assert!(rendered.contains("default-mysql-client"));
    assert!(rendered.contains("ghostscript"));
    assert!(rendered.contains("/usr/local/bin/mysqldump"));
    assert!(rendered.contains("/usr/local/bin/mysql"));
    // Podman 4.9's builder has no heredoc support.
    assert!(!rendered.contains("<<"), "{rendered}");
    assert!(rendered.contains("--column-statistics=0|--set-gtid-purged=OFF"));
    assert!(rendered.contains("--ssl-mode=DISABLED"));
    assert!(rendered.contains("postgresql-client"));
    assert!(rendered.contains("memory_limit=4096M"));
}

#[test]
fn derived_dockerfile_uses_mariadb_client_when_requested() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Node,
        VersionManager::System,
        None,
        SqlClientFlavor::Mariadb,
        None,
    );
    assert!(rendered.contains("mariadb-client"));
    assert!(!rendered.contains("default-mysql-client"));
}

#[test]
fn derived_dockerfile_can_install_fnm_managed_node() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Node,
        VersionManager::Fnm,
        Some("22"),
        SqlClientFlavor::Mysql,
        None,
    );

    assert!(rendered.contains("fnm.vercel.app/install"));
    assert!(rendered.contains("fnm install 22"));
    assert!(rendered.contains("npm install -g pnpm yarn"));
    assert!(!rendered.contains("corepack"));
}

#[test]
fn derived_dockerfile_can_install_deno_runtime() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Deno,
        VersionManager::System,
        Some("2.2.3"),
        SqlClientFlavor::Mysql,
        None,
    );

    assert!(rendered.contains("https://deno.land/install.sh"));
    assert!(rendered.contains("--version 2.2.3"));
    assert!(rendered.contains("/usr/local/bin/deno"));
    assert!(!rendered.contains("https://bun.sh/install"));
    assert!(!rendered.contains("deb.nodesource.com"));
}

#[test]
fn derived_dockerfile_can_install_bun_runtime() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Bun,
        VersionManager::System,
        Some("1.2.5"),
        SqlClientFlavor::Mysql,
        None,
    );

    assert!(rendered.contains("https://bun.sh/install"));
    assert!(rendered.contains("bun-v1.2.5"));
    assert!(rendered.contains("/usr/local/bin/bun"));
    assert!(!rendered.contains("deb.nodesource.com"));
    assert!(!rendered.contains("deno.land/install.sh"));
}

#[test]
fn derived_dockerfile_installs_latest_bun_without_latest_alias() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Bun,
        VersionManager::System,
        None,
        SqlClientFlavor::Mysql,
        None,
    );

    assert!(rendered.contains("https://bun.sh/install"));
    assert!(!rendered.contains("bash -s -- latest"));
    assert!(!rendered.contains("bun-linux-aarch64.zip"));
}

#[test]
fn derived_dockerfile_can_install_playwright_system_dependencies() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        JavaScriptRuntime::Node,
        VersionManager::System,
        Some("22"),
        SqlClientFlavor::Mysql,
        Some("playwright@^1.60.0"),
    );

    assert!(rendered.contains("npx -y 'playwright@^1.60.0' install-deps chromium"));
}

#[test]
fn normalize_php_extensions_maps_sqlite_to_sqlite3_and_dedupes() {
    let normalized = normalize_php_extensions(&[
        "sqlite".to_owned(),
        "pdo_mysql".to_owned(),
        "sqlite3".to_owned(),
    ]);

    assert_eq!(
        normalized,
        vec!["sqlite3".to_owned(), "pdo_mysql".to_owned()]
    );
}

#[test]
fn injects_server_name_for_frankenphp_http_targets() {
    let service = app_service();
    let injected = HashMap::new();
    assert!(should_inject_frankenphp_server_name(&service, &injected));
}

#[test]
fn does_not_inject_server_name_when_already_defined() {
    let service = app_service();
    let mut injected = HashMap::new();
    injected.insert("SERVER_NAME".to_owned(), ":80".to_owned());
    assert!(!should_inject_frankenphp_server_name(&service, &injected));
}

#[test]
fn derived_js_tooling_never_installs_under_root_home() {
    // App containers run as the invoking user, who cannot reach `/root`.
    for (runtime, manager) in [
        (JavaScriptRuntime::Node, VersionManager::System),
        (JavaScriptRuntime::Node, VersionManager::Fnm),
        (JavaScriptRuntime::Node, VersionManager::Nvm),
        (JavaScriptRuntime::Node, VersionManager::Volta),
        (JavaScriptRuntime::Bun, VersionManager::System),
        (JavaScriptRuntime::Deno, VersionManager::System),
    ] {
        let rendered = render_derived_dockerfile(
            "dunglas/frankenphp:php8.5",
            &Vec::new(),
            true,
            runtime,
            manager,
            Some("22"),
            SqlClientFlavor::Mysql,
            Some("playwright@1.50.0"),
        );
        assert!(
            !rendered.contains("/root"),
            "{runtime:?}/{manager:?}:\n{rendered}"
        );
        assert!(
            rendered.contains("ENV BUN_INSTALL=/usr/local/bun BUN_INSTALL_CACHE_DIR=/home/berth/.bun/install/cache DENO_INSTALL=/usr/local/deno FNM_DIR=/usr/local/fnm"),
            "{rendered}"
        );
        if manager != VersionManager::System {
            assert!(
                rendered.contains("chmod -R a+rwX /usr/local/"),
                "{manager:?}"
            );
        }
    }
}
