use super::*;

#[test]
fn project_dependency_injected_env_resolves_base_url_from_domain() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let workspace = std::env::temp_dir().join(format!("berth-project-deps-env-{nonce}"));
    let api_root = workspace.join("api");
    let location_root = workspace.join("location");
    std::fs::create_dir_all(&api_root)?;
    std::fs::create_dir_all(&location_root)?;

    crate::config::project_file::write_test_config_file(
        api_root.join(".berth.yaml"),
        "project_type = \"project\"\ncontainer_prefix = \"api\"\n",
    )?;
    crate::config::project_file::write_test_config_file(
        location_root.join(".berth.yaml"),
        r#"
project_type = "project"
container_prefix = "location"

[[service]]
preset = "laravel"
domain = "acme-location.berth.localhost"
"#,
    )?;
    crate::config::project_file::write_test_config_file(
        workspace.join(".berth.yaml"),
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
    )?;

    let injected = resolve_project_dependency_injected_env(&api_root, None)?;
    let url = injected.get("LOCATION_API_BASE_URL").expect("injected url");
    assert!(
        url.starts_with("http://berth_location_location-") && url.contains("_app:"),
        "an in-network endpoint, not a route the container cannot call: {url}"
    );

    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn project_dependency_injected_env_resolves_when_workspace_config_is_project_root() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let api_root = std::env::temp_dir().join(format!("berth-project-root-workspace-{nonce}"));
    let location_root = api_root.join("location");
    std::fs::create_dir_all(&location_root)?;

    crate::config::project_file::write_test_config_file(
        api_root.join(".berth.yaml"),
        r#"
project_type = "project"
container_prefix = "api"

[[swarm]]
name = "api"
root = "./"
depends_on = ["location"]

[[swarm.inject_env]]
env = "LOCATION_API_BASE_URL"
from = "location"
value = ":base_url"

[[swarm]]
name = "location"
root = "location"
"#,
    )?;
    crate::config::project_file::write_test_config_file(
        location_root.join(".berth.yaml"),
        r#"
project_type = "project"
container_prefix = "location"

[[service]]
preset = "laravel"
domain = "acme-location.berth.localhost"
"#,
    )?;

    let injected = resolve_project_dependency_injected_env(&api_root, None)?;
    let url = injected.get("LOCATION_API_BASE_URL").expect("injected url");
    assert!(
        url.starts_with("http://berth_location_location-") && url.ends_with("_app:80"),
        "{url}"
    );

    std::fs::remove_dir_all(api_root)?;
    Ok(())
}

#[test]
fn injected_tokens_name_the_source_container_on_its_network() -> Result<()> {
    let mut service = crate::config::preset_preview("laravel")?;
    service.resolved_container_name = Some("berth_location_a-1_app".to_owned());
    let port = service.resolved_container_port();
    assert_eq!(
        resolve_injected_env_value(":host", &service)?,
        "berth_location_a-1_app"
    );
    assert_eq!(
        resolve_injected_env_value(":port", &service)?,
        port.to_string()
    );
    assert_eq!(resolve_injected_env_value(":scheme", &service)?, "http");
    for token in [":url", ":base_url"] {
        assert_eq!(
            resolve_injected_env_value(token, &service)?,
            format!("http://berth_location_a-1_app:{port}")
        );
    }
    assert_eq!(resolve_injected_env_value("literal", &service)?, "literal");
    assert!(resolve_injected_env_value(":nope", &service).is_err());
    Ok(())
}

#[test]
fn project_dependency_injected_env_is_empty_outside_swarm_workspace() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("berth-project-no-workspace-{nonce}"));
    std::fs::create_dir_all(&root)?;
    crate::config::project_file::write_test_config_file(
        root.join(".berth.yaml"),
        "project_type = \"project\"\ncontainer_prefix = \"solo\"\n",
    )?;

    let injected = resolve_project_dependency_injected_env(&root, None)?;
    assert!(injected.is_empty());

    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn project_dependency_runner_is_noop_outside_swarm_workspace() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("berth-project-deps-runner-noop-{nonce}"));
    std::fs::create_dir_all(&root)?;
    crate::config::project_file::write_test_config_file(
        root.join(".berth.yaml"),
        "project_type = \"project\"\ncontainer_prefix = \"solo\"\n",
    )?;

    let result = run_project_swarm_dependencies(crate::swarm::RunProjectSwarmDependenciesOptions {
        operation: "up",
        project_root: &root,
        quiet: true,
        no_color: true,
        dry_run: true,
        runtime_env: None,
        force_down_deps: false,
    });

    std::fs::remove_dir_all(root)?;
    assert!(
        result.is_ok(),
        "expected standalone project dependency run to noop"
    );
    Ok(())
}
