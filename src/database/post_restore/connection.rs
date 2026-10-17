//! database post restore connection module.
//!
//! Resolves the Laravel connection name that post-restore artisan hooks
//! should target for a restored SQL service.

use anyhow::{Context, Result};
use std::path::Path;

use crate::config::ServiceConfig;

pub(super) struct PostRestoreConnection {
    pub(super) name: String,
}

pub(super) fn resolve_post_restore_connection(
    project_root: &Path,
    restored_service: &ServiceConfig,
) -> Result<PostRestoreConnection> {
    let driver_connection = restored_service
        .laravel_connection()
        .map(str::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "service '{}' does not map to a Laravel database driver",
                restored_service.name
            )
        })?;

    let database_env = restored_service
        .env_mapping
        .as_ref()
        .and_then(|mapping| mapping.get("DB_DATABASE"))
        .map_or("DB_DATABASE", String::as_str);

    if database_env == "DB_DATABASE" {
        return Ok(PostRestoreConnection {
            name: driver_connection,
        });
    }

    let config_path = project_root.join("config/database.php");
    let content = std::fs::read_to_string(&config_path).with_context(|| {
        format!(
            "failed to read Laravel database config at {}",
            config_path.display()
        )
    })?;

    let connection_name = find_connection_name(&content, database_env, driver_connection.as_str())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "failed to resolve Laravel connection for restored service '{}' \
from env '{}' in {}",
                restored_service.name,
                database_env,
                config_path.display()
            )
        })?;

    Ok(PostRestoreConnection {
        name: connection_name,
    })
}

fn find_connection_name(content: &str, database_env: &str, driver: &str) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let connections_index = lines
        .iter()
        .position(|line| line.contains("'connections'") && line.contains('['))?;

    let mut depth = bracket_delta(lines[connections_index]);
    let mut current_name: Option<String> = None;
    let mut current_body = String::new();

    for line in lines.iter().skip(connections_index + 1) {
        let trimmed = line.trim();

        if depth == 1
            && current_name.is_none()
            && let Some(name) = connection_name_from_line(trimmed)
        {
            current_name = Some(name);
            current_body.clear();
        }

        if current_name.is_some() {
            current_body.push_str(trimmed);
            current_body.push('\n');
        }

        depth += bracket_delta(trimmed);

        if depth == 1
            && let Some(name) = current_name.take()
            && block_matches(current_body.as_str(), database_env, driver)
        {
            return Some(name);
        }

        if depth == 0 {
            break;
        }
    }

    None
}

fn connection_name_from_line(line: &str) -> Option<String> {
    let (quoted, _) = line.split_once("' => [")?;
    let name = quoted.strip_prefix('\'')?;
    Some(name.to_owned())
}

fn block_matches(block: &str, database_env: &str, driver: &str) -> bool {
    block.contains(&format!("'driver' => '{driver}'"))
        && block.contains(&format!("env('{database_env}'"))
}

fn bracket_delta(line: &str) -> i32 {
    let opens = line.matches('[').count();
    let closes = line.matches(']').count();
    i32::try_from(opens).unwrap_or(0) - i32::try_from(closes).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{find_connection_name, resolve_post_restore_connection};
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn mysql_service(name: &str) -> ServiceConfig {
        ServiceConfig {
            name: name.to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: "mysql:8.0".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("laravel".to_owned()),
            username: Some("laravel".to_owned()),
            password: Some("laravel".to_owned()),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: None,
            resolved_container_name: Some(format!("test-{name}")),
            runtime: Default::default(),
        }
    }

    fn with_database_config<F, T>(content: &str, test: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let root = env::temp_dir().join(format!(
            "berth-post-restore-connection-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("config")).expect("create config directory");
        fs::write(root.join("config/database.php"), content).expect("write database config");
        let result = test(&root);
        fs::remove_dir_all(&root).ok();
        result
    }

    #[test]
    fn resolve_post_restore_connection_uses_driver_name_for_primary_database() {
        with_database_config("<?php return [];", |project_root| {
            let service = mysql_service("shipit");
            let connection = resolve_post_restore_connection(project_root, &service)
                .expect("resolve primary connection");
            assert_eq!(connection.name, "mysql");
        });
    }

    #[test]
    fn resolve_post_restore_connection_finds_named_connection_from_database_config() {
        with_database_config(
            r#"<?php
return [
    'connections' => [
        'mysql' => [
            'driver' => 'mysql',
            'database' => env('DB_DATABASE', 'forge'),
        ],
        'shipit_invoicing' => [
            'driver' => 'mysql',
            'database' => env('DB_INVOICING_DATABASE', ''),
        ],
    ],
];
"#,
            |project_root| {
                let mut service = mysql_service("billing");
                service.env_mapping = Some(HashMap::from([(
                    "DB_DATABASE".to_owned(),
                    "DB_INVOICING_DATABASE".to_owned(),
                )]));
                let connection = resolve_post_restore_connection(project_root, &service)
                    .expect("resolve mapped connection");
                assert_eq!(connection.name, "shipit_invoicing");
            },
        );
    }

    #[test]
    fn find_connection_name_matches_driver_and_database_env() {
        let content = r#"<?php
return [
    'connections' => [
        'mysql' => [
            'driver' => 'mysql',
            'database' => env('DB_DATABASE', 'forge'),
        ],
        'shipit_invoicing' => [
            'driver' => 'mysql',
            'database' => env('DB_INVOICING_DATABASE', ''),
        ],
    ],
];
"#;

        assert_eq!(
            find_connection_name(content, "DB_INVOICING_DATABASE", "mysql"),
            Some("shipit_invoicing".to_owned())
        );
    }
}
