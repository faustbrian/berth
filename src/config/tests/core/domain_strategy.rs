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
    let path = root.join(".berth.toml");
    fs::write(&path, content).expect("write config");
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

#[test]
fn load_config_with_random_strategy_is_stable_for_same_project_root() {
    let root = temp_root("random-stable");
    let config_path = write_config(
        &root,
        r#"
schema_version = 1
project_type = "project"
container_prefix = "shipit-api"
domain_strategy = "random"

[[service]]
preset = "laravel"

[[service]]
preset = "mailhog"
"#,
    );

    let first =
        load_config_with(LoadConfigPathOptions::new(Some(&config_path), None)).expect("first load");
    let second = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("second load");

    let first_app = first
        .service
        .iter()
        .find(|service| service.name == "app")
        .expect("app service");
    let second_app = second
        .service
        .iter()
        .find(|service| service.name == "app")
        .expect("app service");
    let second_mailhog = second
        .service
        .iter()
        .find(|service| service.name == "mailhog")
        .expect("mailhog service");

    let base = first_app.primary_domain().expect("generated domain");
    assert_eq!(Some(base), second_app.primary_domain());
    assert!(base.starts_with("berth-"));
    assert!(base.ends_with(".berth"));
    let expected_mailhog = format!("{}-mailhog.berth", base.trim_end_matches(".berth"));
    assert_eq!(
        second_mailhog.primary_domain(),
        Some(expected_mailhog.as_str())
    );
}

#[test]
fn load_config_with_random_strategy_differs_for_different_project_roots() {
    let first_root = temp_root("random-one");
    let second_root = temp_root("random-two");
    let first_path = write_config(
        &first_root,
        r#"
schema_version = 1
project_type = "project"
container_prefix = "shipit-api"
domain_strategy = "random"

[[service]]
preset = "laravel"
"#,
    );
    let second_path = write_config(
        &second_root,
        r#"
schema_version = 1
project_type = "project"
container_prefix = "shipit-api"
domain_strategy = "random"

[[service]]
preset = "laravel"
"#,
    );

    let first =
        load_config_with(LoadConfigPathOptions::new(Some(&first_path), None)).expect("first load");
    let second = load_config_with(LoadConfigPathOptions::new(Some(&second_path), None))
        .expect("second load");

    assert_ne!(
        first.service[0].primary_domain(),
        second.service[0].primary_domain()
    );
}
