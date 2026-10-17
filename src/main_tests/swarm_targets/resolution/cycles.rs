use super::*;

#[test]
fn resolve_swarm_targets_rejects_dependency_cycles() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let base = std::env::temp_dir().join(format!("berth-swarm-cycle-test-{nonce}"));
    std::fs::create_dir_all(base.join("api"))?;
    std::fs::create_dir_all(base.join("bill"))?;
    std::fs::write(base.join("api/.berth.toml"), "container_prefix = \"api\"\n")?;
    std::fs::write(
        base.join("bill/.berth.toml"),
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
                depends_on: vec!["bill".to_owned()],
                inject_env: vec![],
                git: None,
            },
            SwarmTarget {
                name: "bill".to_owned(),
                root: PathBuf::from("bill"),
                depends_on: vec!["api".to_owned()],
                inject_env: vec![],
                git: None,
            },
        ],
    };

    let error = match resolve_swarm_targets(&config, &base, &[String::from("api")], true) {
        Ok(_) => anyhow::bail!("expected dependency cycle error"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("circular swarm dependency"));

    std::fs::remove_dir_all(base)?;
    Ok(())
}
