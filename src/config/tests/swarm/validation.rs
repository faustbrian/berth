use super::*;

#[test]
fn load_config_with_rejects_duplicate_swarm_target_names() {
    let nonce = crate::test_support::unique_stamp();
    let root = crate::test_support::temp_root().join(format!("berth-config-swarm-dup-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "api"
                root = "api"

                [[swarm]]
                name = "api"
                root = "bill"
            "#,
    )
    .expect("write swarm config");

    let error = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect_err("expected duplicate error");
    assert!(format!("{error:#}").contains("duplicate swarm target name"));

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}

#[test]
fn load_config_with_rejects_unknown_swarm_dependency() {
    let nonce = crate::test_support::unique_stamp();
    let root = crate::test_support::temp_root().join(format!("berth-config-swarm-dep-{nonce}"));
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
            "#,
    )
    .expect("write swarm config");

    let error = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect_err("expected dep error");
    assert!(format!("{error:#}").contains("depends on unknown target 'bill'"));

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}

#[test]
fn load_config_with_rejects_empty_swarm_git_repo() {
    let nonce = crate::test_support::unique_stamp();
    let root =
        crate::test_support::temp_root().join(format!("berth-config-swarm-git-repo-{nonce}"));
    std::fs::create_dir_all(&root).expect("create temp config directory");

    let config_path = root.join(".berth.yaml");
    project_file::write_test_config_file(
        &config_path,
        r#"
                project_type = "project"
                [[swarm]]
                name = "api"
                root = "api"

                [[swarm.git]]
                repo = "   "
            "#,
    )
    .expect("write swarm config");

    let error = load_config_with(LoadConfigPathOptions::new(Some(&config_path), None))
        .expect_err("expected git repo error");
    assert!(format!("{error:#}").contains("empty git.repo"));

    std::fs::remove_dir_all(root).expect("cleanup temp config directory");
}
