use super::*;

#[test]
fn resolve_swarm_targets_filters_only_and_validates_members() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = crate::test_support::temp_root().join(format!("berth-swarm-test-{nonce}"));
    std::fs::create_dir_all(base.join("api"))?;
    std::fs::create_dir_all(base.join("bill"))?;
    crate::config::project_file::write_test_config_file(
        base.join("api/.berth.yaml"),
        "container_prefix = \"api\"\n",
    )?;
    crate::config::project_file::write_test_config_file(
        base.join("bill/.berth.yaml"),
        "container_prefix = \"bill\"\n",
    )?;

    let config = Config {
        schema_version: 1,
        project_type: crate::config::ProjectType::Project,
        container_prefix: None,
        domain_strategy: None,
        service: Vec::new(),
        swarm: vec![
            SwarmTarget {
                name: "api".to_owned(),
                root: PathBuf::from("api"),
                depends_on: vec![],
                inject_env: vec![],
                git: None,
            },
            SwarmTarget {
                name: "bill".to_owned(),
                root: PathBuf::from("bill"),
                depends_on: vec![],
                inject_env: vec![],
                git: None,
            },
        ],
    };

    let targets = resolve_swarm_targets(&config, &base, &[String::from("bill")], true)?;
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].name, "bill");
    assert!(targets[0].root.ends_with("bill"));

    std::fs::remove_dir_all(base)?;
    Ok(())
}
