use super::*;

#[test]
fn down_guard_blocks_shared_implicit_dependencies() -> Result<()> {
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
                name: "other".to_owned(),
                root: PathBuf::from("other"),
                depends_on: vec!["bill".to_owned()],
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

    let expanded = vec![
        ResolvedSwarmTarget {
            name: "bill".to_owned(),
            root: PathBuf::from("/tmp/ws/bill"),
        },
        ResolvedSwarmTarget {
            name: "api".to_owned(),
            root: PathBuf::from("/tmp/ws/api"),
        },
    ];

    let error = enforce_shared_down_dependency_guard(
        &config,
        &[String::from("api")],
        &expanded,
        false,
        &PathBuf::from("/tmp/ws"),
    )
    .expect_err("expected shared dependency guard failure");
    assert!(
        error
            .to_string()
            .contains("run the same command again with `--force`")
    );

    Ok(())
}
