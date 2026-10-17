use super::helpers::{app_service, mysql_service};
use super::*;

#[test]
fn resolve_app_requires_name_when_multiple_targets() {
    let config = Config {
        schema_version: 1,
        project_type: ProjectType::Project,
        container_prefix: Some("test".to_owned()),
        domain_strategy: None,
        service: vec![
            mysql_service("db1"),
            app_service("web1", "one.berth", 8000, Driver::Frankenphp),
            app_service("web2", "two.berth", 8001, Driver::Frankenphp),
        ],
        swarm: vec![],
    };

    let result = resolve_app_service(&config, None);
    assert!(result.is_err());
}

#[test]
fn resolve_app_defaults_to_named_app_when_multiple_targets() {
    let mut mailhog = app_service("mailhog", "mailhog.berth", 8025, Driver::Mailhog);
    mailhog.container_port = Some(8025);
    mailhog.smtp_port = Some(1025);

    let config = Config {
        schema_version: 1,
        project_type: ProjectType::Project,
        container_prefix: Some("test".to_owned()),
        domain_strategy: None,
        service: vec![
            app_service("app", "app.berth", 8000, Driver::Frankenphp),
            mailhog,
        ],
        swarm: vec![],
    };

    let resolved = resolve_app_service(&config, None).expect("resolved app default");
    assert_eq!(resolved.name, "app");
}
