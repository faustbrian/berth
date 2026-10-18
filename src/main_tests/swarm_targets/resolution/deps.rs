use super::*;

#[test]
fn resolve_swarm_targets_includes_dependencies_by_default() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = crate::test_support::temp_root().join(format!("berth-swarm-deps-test-{nonce}"));
    std::fs::create_dir_all(base.join("api"))?;
    std::fs::create_dir_all(base.join("bill"))?;
    std::fs::create_dir_all(base.join("postal"))?;
    crate::config::project_file::write_test_config_file(
        base.join("api/.berth.yaml"),
        "container_prefix = \"api\"\n",
    )?;
    crate::config::project_file::write_test_config_file(
        base.join("bill/.berth.yaml"),
        "container_prefix = \"bill\"\n",
    )?;
    crate::config::project_file::write_test_config_file(
        base.join("postal/.berth.yaml"),
        "container_prefix = \"postal\"\n",
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
                depends_on: vec!["bill".to_owned()],
                inject_env: vec![],
                git: None,
            },
            SwarmTarget {
                name: "bill".to_owned(),
                root: PathBuf::from("bill"),
                depends_on: vec!["postal".to_owned()],
                inject_env: vec![],
                git: None,
            },
            SwarmTarget {
                name: "postal".to_owned(),
                root: PathBuf::from("postal"),
                depends_on: vec![],
                inject_env: vec![],
                git: None,
            },
        ],
    };

    let with_deps = resolve_swarm_targets(&config, &base, &[String::from("api")], true)?;
    assert_eq!(
        with_deps
            .iter()
            .map(|target| target.name.as_str())
            .collect::<Vec<_>>(),
        vec!["postal", "bill", "api"]
    );

    let without_deps = resolve_swarm_targets(&config, &base, &[String::from("api")], false)?;
    assert_eq!(
        without_deps
            .iter()
            .map(|target| target.name.as_str())
            .collect::<Vec<_>>(),
        vec!["api"]
    );

    std::fs::remove_dir_all(base)?;
    Ok(())
}
