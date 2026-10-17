use super::*;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_root(prefix: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "berth-domain-strategy-{prefix}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos()
    ));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("create temp root");
    root
}

fn write_config(root: &std::path::Path, content: &str) -> PathBuf {
    let path = root.join(".berth.yaml");
    project_file::write_test_config_file(&path, content).expect("write config");
    path
}

#[test]
fn load_config_with_directory_strategy_generates_app_domains_from_project_root() {
    let root = temp_root("directory-parent").join("my-project");
    fs::create_dir_all(&root).expect("create project root");
    let config_path = write_config(
        &root,
        r#"
schema_version = 1
project_type = "project"
container_prefix = "shipit-api"
domain_strategy = "directory"

[[service]]
preset = "laravel"

[[service]]
preset = "gotenberg"

[[service]]
preset = "mailhog"
"#,
    );

    let config = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("load config");

    let app = config
        .service
        .iter()
        .find(|service| service.name == "app")
        .expect("app service");
    let gotenberg = config
        .service
        .iter()
        .find(|service| service.name == "gotenberg")
        .expect("gotenberg service");
    let mailhog = config
        .service
        .iter()
        .find(|service| service.name == "mailhog")
        .expect("mailhog service");

    assert_eq!(app.primary_domain(), Some("my-project.berth"));
    assert_eq!(
        gotenberg.primary_domain(),
        Some("my-project-gotenberg.berth")
    );
    assert_eq!(mailhog.primary_domain(), Some("my-project-mailhog.berth"));
    assert_eq!(app.domain, None);
    assert_eq!(gotenberg.domain, None);
    assert_eq!(mailhog.domain, None);
}
