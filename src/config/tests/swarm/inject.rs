use super::*;

#[test]
fn load_config_with_parses_swarm_injected_env_rules() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("berth-config-swarm-inject-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "api"
                root = "api"
                depends_on = ["location"]

                [[swarm.inject_env]]
                env = "LOCATION_API_BASE_URL"
                from = "location"
                value = ":base_url"

                [[swarm]]
                name = "location"
                root = "location"
            "#,
    )
    .expect("write swarm config");

    let loaded = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("load swarm config");
    assert_eq!(loaded.swarm.len(), 2);
    assert_eq!(loaded.swarm[0].inject_env.len(), 1);
    assert_eq!(loaded.swarm[0].inject_env[0].env, "LOCATION_API_BASE_URL");
    assert_eq!(loaded.swarm[0].inject_env[0].from, "location");
    assert_eq!(loaded.swarm[0].inject_env[0].value, ":base_url");

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}

#[test]
fn load_config_with_rejects_unknown_swarm_inject_token() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("berth-config-swarm-inject-token-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "api"
                root = "api"
                depends_on = ["location"]

                [[swarm.inject_env]]
                env = "LOCATION_API_BASE_URL"
                from = "location"
                value = ":hostname"

                [[swarm]]
                name = "location"
                root = "location"
            "#,
    )
    .expect("write swarm config");

    let error = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect_err("expected token error");
    assert!(error.to_string().contains("unsupported token ':hostname'"));

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}
