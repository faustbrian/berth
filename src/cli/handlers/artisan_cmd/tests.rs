//! cli handlers artisan cmd tests module.
//!
//! Contains cli handlers artisan cmd tests logic used by Berth command workflows.

use super::{
    HandleArtisanOptions, build_artisan_command, build_artisan_test_command, handle_artisan,
    is_artisan_test_command, load_artisan_test_base_config, remove_artisan_env_overrides,
    resolve_artisan_tty,
};
use crate::config;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[test]
fn detects_artisan_test_subcommand() {
    assert!(is_artisan_test_command(&["test".to_owned()]));
    assert!(is_artisan_test_command(&[
        "TeSt".to_owned(),
        "--filter=Foo".to_owned()
    ]));
    assert!(!is_artisan_test_command(&["migrate".to_owned()]));
    assert!(!is_artisan_test_command(&[]));
}

/// Removes artisan env overrides strips only env options as part of the cli handlers artisan cmd tests workflow.
#[test]
fn remove_artisan_env_overrides_strips_only_env_options() {
    let command = vec![
        "test".to_owned(),
        "--env=local".to_owned(),
        "--filter=Invoice".to_owned(),
        "--env".to_owned(),
        "staging".to_owned(),
        "--verbose".to_owned(),
    ];

    assert_eq!(
        remove_artisan_env_overrides(&command),
        vec![
            "test".to_owned(),
            "--filter=Invoice".to_owned(),
            "--verbose".to_owned(),
        ]
    );
}

#[test]
fn artisan_tty_respects_explicit_flags() {
    assert!(!resolve_artisan_tty(false, true));
    assert_eq!(
        resolve_artisan_tty(true, false),
        crate::cli::support::effective_tty(false, false)
    );
}

#[test]
fn artisan_tty_uses_effective_tty_default_path() {
    assert_eq!(
        crate::cli::support::effective_tty(false, false),
        resolve_artisan_tty(false, false)
    );
}

#[test]
fn artisan_test_command_sets_memory_limit_to_4096m() {
    let mut inferred_env = HashMap::new();
    inferred_env.insert("DB_HOST".to_owned(), "host.docker.internal".to_owned());

    let command = build_artisan_test_command(vec!["test".to_owned()], &inferred_env, false);
    assert_eq!(command[0], "sh");
    assert_eq!(command[1], "-lc");
    assert!(command[2].contains("memory_limit=4096M"));
    assert!(command[2].contains("export PHP_INI_SCAN_DIR="));
    assert!(command[2].contains("export DB_HOST='host.docker.internal'"));
    assert!(command[2].contains("export APP_ENV='testing'"));
    assert!(command[2].contains("'php' 'artisan' 'test'"));
}

/// Builds non test artisan command uses plain php invocation for command execution.
#[test]
fn build_non_test_artisan_command_uses_plain_php_invocation() {
    assert_eq!(
        build_artisan_command(vec!["about".to_owned()]),
        vec!["php".to_owned(), "artisan".to_owned(), "about".to_owned(),]
    );
}

#[test]
fn loads_testing_config_when_present_and_falls_back_to_default() {
    let root = temp_root("testing-config-preferred");
    let default_service = service_toml("default");
    let testing_service = service_toml("testing");

    config::project_file::write_test_config_file(
            root.join(".berth.yaml"),
        format!(
            "schema_version = 1\nproject_type = \"project\"\ncontainer_prefix = \"test\"\nservice = [{default_service}]\nswarm = []\n"
        ),
    )
    .expect("write default config");
    config::project_file::write_test_config_file(
            root.join(".berth.testing.yaml"),
        format!(
            "schema_version = 1\nproject_type = \"project\"\ncontainer_prefix = \"test\"\nservice = [{testing_service}]\nswarm = []\n"
        ),
    )
    .expect("write testing config");

    let loaded = load_artisan_test_base_config(None, None, &root).expect("load testing config");
    assert_eq!(loaded.service[0].name, "testing");

    fs::remove_file(root.join(".berth.testing.yaml")).expect("remove testing config");
    let fallback = load_artisan_test_base_config(None, None, &root).expect("load fallback config");
    assert_eq!(fallback.service[0].name, "default");
}

#[test]
fn handle_artisan_rejects_library_project_type() {
    let config = config::Config {
        schema_version: 1,
        project_type: config::ProjectType::Library,
        container_prefix: None,
        domain_strategy: None,
        service: Vec::new(),
        swarm: Vec::new(),
    };

    let result = handle_artisan(
        &config,
        HandleArtisanOptions {
            service: None,
            kind: None,
            profile: None,
            non_interactive: true,
            browser: false,
            tty: false,
            no_tty: true,
            command: &[],
            config_path: None,
            project_root: None,
        },
    );

    assert!(result.is_err());
    assert!(
        result
            .expect_err("library mode should reject artisan")
            .to_string()
            .contains("project_type is \"project\"")
    );
}

fn temp_root(name: &str) -> PathBuf {
    let root = crate::test_support::temp_root().join(format!(
        "berth-artisan-cfg-{name}-{}",
        crate::test_support::unique_stamp()
    ));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn service_toml(name: &str) -> String {
    format!(
        r#"{{
  name = "{name}",
  kind = "app",
  driver = "frankenphp",
  image = "dunglas/frankenphp",
  host = "127.0.0.1",
  port = 8080
}}"#
    )
}
