use super::*;
use std::path::PathBuf;

#[test]
fn load_config_with_parses_swarm_targets() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("berth-config-swarm-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "api"
                root = "api"
                depends_on = ["bill"]

                [[swarm]]
                name = "bill"
                root = "./bill"
            "#,
    )
    .expect("write swarm config");

    let loaded = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("load swarm config");
    assert_eq!(loaded.swarm.len(), 2);
    assert_eq!(loaded.swarm[0].name, "api");
    assert_eq!(loaded.swarm[0].root, PathBuf::from("api"));
    assert_eq!(loaded.swarm[0].depends_on, vec![String::from("bill")]);
    assert!(loaded.swarm[0].inject_env.is_empty());

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}

#[test]
fn load_config_with_parses_swarm_git_target() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("berth-config-swarm-git-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "rate"
                root = "rate"

                [[swarm.git]]
                repo = "git@github.com:acmefi/rate.git"
                branch = "develop"
            "#,
    )
    .expect("write swarm config");

    let loaded = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect("load swarm config");
    assert_eq!(loaded.swarm.len(), 1);
    assert_eq!(loaded.swarm[0].name, "rate");
    assert_eq!(
        loaded.swarm[0].git.as_ref().expect("git").repo,
        "git@github.com:acmefi/rate.git"
    );
    assert_eq!(
        loaded.swarm[0].git.as_ref().expect("git").branch.as_deref(),
        Some("develop")
    );

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}
