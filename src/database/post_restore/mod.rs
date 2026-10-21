//! database post restore module.
//!
//! Contains database post restore logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};

mod connection;
mod options;

pub(crate) use options::PostRestoreOptions;

use connection::resolve_post_restore_connection;

/// Runs optional post-restore artisan hooks for the restored database service.
pub(crate) fn run_laravel_post_restore(options: PostRestoreOptions<'_>) -> Result<()> {
    let project_root =
        crate::config::project_root_with(crate::config::ProjectRootPathOptions::new(
            options.config_path,
            options.project_root_override,
        ))?;

    if options.run_migrate {
        run_artisan_command(
            &project_root,
            options.config_path,
            options.restored_service,
            "migrate",
        )?;
    }

    if options.run_schema_dump {
        run_artisan_command(
            &project_root,
            options.config_path,
            options.restored_service,
            "schema:dump",
        )?;
    }

    Ok(())
}

/// Runs a single artisan command via `berth artisan`.
fn run_artisan_command(
    project_root: &Path,
    config_path: Option<&Path>,
    restored_service: &crate::config::ServiceConfig,
    artisan_command: &str,
) -> Result<()> {
    let connection = resolve_post_restore_connection(project_root, restored_service)?;

    output::event(
        "database",
        LogLevel::Info,
        &running_message(
            artisan_command,
            restored_service.name.as_str(),
            connection.name.as_str(),
        ),
        Persistence::Persistent,
    );

    if crate::docker::is_dry_run() {
        output::event(
            "database",
            LogLevel::Info,
            &dry_run_message(
                artisan_command,
                restored_service.name.as_str(),
                connection.name.as_str(),
            ),
            Persistence::Transient,
        );
        return Ok(());
    }

    // The app of the environment the restore went into: the run's config
    // file and runtime environment, as the command itself loaded them (the
    // instance is bound for the whole process).
    let runtime_env = restored_service.runtime.env.as_deref();
    let mut config = crate::config::load_config_with(
        crate::config::LoadConfigPathOptions::new(
            config_path,
            config_path.is_none().then_some(project_root),
        )
        .with_runtime_env(runtime_env),
    )
    .context("failed to load berth config for artisan post-restore hook")?;
    if let Some(runtime_env) = runtime_env {
        crate::config::apply_runtime_env(&mut config, runtime_env)?;
    }
    let app_service = crate::config::resolve_app_service(&config, None).with_context(|| {
        format!(
            "the restore into '{}' succeeded, but berth cannot pick the app service to run \
             `artisan {artisan_command}` in; run it yourself with {}",
            restored_service.name,
            crate::followup::for_service(
                restored_service,
                &format!(
                    "artisan --service <app> {artisan_command} --database={}",
                    connection.name
                )
            )
        )
    })?;
    let args = vec![
        artisan_command.to_owned(),
        format!("--database={}", connection.name),
    ];
    crate::serve::exec_artisan(app_service, &args, false).with_context(|| {
        format!(
            "the restore into '{}' succeeded, but `artisan {artisan_command} --database={}` \
             failed; fix the error above and rerun it alone, without restoring again: {}",
            restored_service.name,
            connection.name,
            crate::followup::for_service(
                app_service,
                &format!(
                    "artisan --service {} {artisan_command} --database={}",
                    app_service.name, connection.name
                )
            )
        )
    })?;

    output::event(
        "database",
        LogLevel::Success,
        &completed_message(
            artisan_command,
            restored_service.name.as_str(),
            connection.name.as_str(),
        ),
        Persistence::Persistent,
    );
    Ok(())
}

fn running_message(artisan_command: &str, service_name: &str, connection_name: &str) -> String {
    format!(
        "Running `berth artisan {artisan_command} --database={connection_name}` \
for restored service `{service_name}`"
    )
}

fn dry_run_message(artisan_command: &str, service_name: &str, connection_name: &str) -> String {
    format!(
        "[dry-run] berth artisan {artisan_command} --database={connection_name} \
for restored service `{service_name}`"
    )
}

fn completed_message(artisan_command: &str, service_name: &str, connection_name: &str) -> String {
    format!(
        "`berth artisan {artisan_command} --database={connection_name}` completed \
for restored service `{service_name}`"
    )
}

#[cfg(test)]
mod tests {
    use super::run_laravel_post_restore;
    use super::{completed_message, dry_run_message, running_message};
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::database::PostRestoreOptions;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn with_temp_project<F, T>(database_php: &str, test: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let root = crate::test_support::temp_root().join(format!(
            "berth-post-restore-{}",
            crate::test_support::unique_stamp()
        ));
        // The clock is coarse: parallel tests could share a name and remove
        // each other's project.
        let root = crate::docker::unique_test_path(root);
        fs::create_dir_all(root.join("config")).expect("create temp project");
        crate::config::project_file::write_test_config_file(
            root.join(".berth.yaml"),
            "schema_version = 1\nproject_type = \"project\"\ncontainer_prefix = \"acme\"\n[[service]]\npreset = \"laravel\"\nname = \"app\"\n",
        )
        .expect("write temp config");
        fs::write(root.join("config/database.php"), database_php).expect("write database config");

        let result = test(&root);
        fs::remove_dir_all(&root).ok();
        result
    }

    fn with_fake_docker_command<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-post-restore-docker-{}",
            crate::test_support::unique_stamp()
        ));
        // A clock tick is coarse: two parallel tests could share a name and
        // remove each other's fake binary.
        let bin_dir = crate::docker::unique_test_path(bin_dir);
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write fake docker");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary)
                .expect("fake docker metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod fake docker");
        }

        let command = binary.to_string_lossy().to_string();
        let result = crate::docker::with_docker_command(&command, test);
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    fn shipit_service() -> ServiceConfig {
        ServiceConfig {
            name: "shipit".to_owned(),
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
            routes: None,
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
            resolved_container_name: Some("acme-shipit".to_owned()),
            runtime: Default::default(),
        }
    }

    fn billing_service() -> ServiceConfig {
        let mut service = shipit_service();
        service.name = "billing".to_owned();
        service.env_mapping = Some(HashMap::from([
            ("DB_HOST".to_owned(), "DB_INVOICING_HOST".to_owned()),
            ("DB_PORT".to_owned(), "DB_INVOICING_PORT".to_owned()),
            ("DB_DATABASE".to_owned(), "DB_INVOICING_DATABASE".to_owned()),
            ("DB_USERNAME".to_owned(), "DB_INVOICING_USERNAME".to_owned()),
            ("DB_PASSWORD".to_owned(), "DB_INVOICING_PASSWORD".to_owned()),
        ]));
        service
    }

    fn database_php() -> &'static str {
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
"#
    }

    fn capture_script(path: &Path) -> String {
        format!("printf '%s\n' \"$*\" >> {}", path.display())
    }

    fn capture_path() -> PathBuf {
        crate::docker::unique_test_path(crate::test_support::temp_root().join(format!(
            "berth-post-restore-capture-{}",
            crate::test_support::unique_stamp()
        )))
    }

    #[test]
    fn run_laravel_post_restore_supports_partial_hook_combinations() {
        with_temp_project(database_php(), |project_root| {
            crate::docker::with_dry_run_lock(|| {
                let service = shipit_service();
                run_laravel_post_restore(PostRestoreOptions::new(
                    false,
                    false,
                    &service,
                    Some(project_root),
                    None,
                ))
                .expect("post-restore skip path");
            });
        });
    }

    #[test]
    fn run_laravel_post_restore_executes_post_restore_with_resolved_connection() {
        with_temp_project(database_php(), |project_root| {
            let capture = capture_path();
            with_fake_docker_command(&capture_script(&capture), || {
                let service = billing_service();
                run_laravel_post_restore(PostRestoreOptions::new(
                    true,
                    true,
                    &service,
                    Some(project_root),
                    None,
                ))
                .expect("post-restore run");
            });

            let recorded = fs::read_to_string(&capture).expect("read capture");
            fs::remove_file(&capture).ok();
            assert!(recorded.contains("php artisan migrate --database=shipit_invoicing"));
            assert!(recorded.contains("php artisan schema:dump --database=shipit_invoicing"));
        });
    }

    #[test]
    fn a_failed_post_restore_under_env_runs_and_reruns_in_that_env() {
        with_temp_project(database_php(), |project_root| {
            let capture = capture_path();
            let script = format!("{}\nexit 1", capture_script(&capture));
            let error = with_fake_docker_command(&script, || {
                let mut service = billing_service();
                service.runtime.env = Some("staging".to_owned());
                run_laravel_post_restore(PostRestoreOptions::new(
                    true,
                    false,
                    &service,
                    Some(project_root),
                    None,
                ))
                .expect_err("artisan fails")
            });
            let recorded = fs::read_to_string(&capture).unwrap_or_default();
            fs::remove_file(&capture).ok();
            let message = format!("{error:#}");
            // The migrate ran in the staging app, and its rerun stays there.
            assert!(recorded.contains("staging"), "{recorded}");
            assert!(message.contains("--env staging"), "{message}");
            assert!(message.contains("without restoring again"), "{message}");
        });
    }

    #[test]
    fn run_laravel_post_restore_dry_run_only_skips_artisan_execution() {
        with_temp_project(database_php(), |project_root| {
            crate::docker::with_dry_run_lock(|| {
                let service = shipit_service();
                run_laravel_post_restore(PostRestoreOptions::new(
                    true,
                    true,
                    &service,
                    Some(project_root),
                    None,
                ))
                .expect("dry-run post-restore");
            });
        });
    }

    #[test]
    fn message_helpers_serialize_expected_artisan_labels() {
        assert_eq!(
            running_message("migrate", "shipit", "mysql"),
            "Running `berth artisan migrate --database=mysql` for restored service `shipit`"
        );
        assert_eq!(
            dry_run_message("schema:dump", "billing", "shipit_invoicing"),
            "[dry-run] berth artisan schema:dump --database=shipit_invoicing \
for restored service `billing`"
        );
        assert_eq!(
            completed_message("config:cache", "shipit", "mysql"),
            "`berth artisan config:cache --database=mysql` completed for restored service `shipit`"
        );
    }
}
