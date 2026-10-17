use super::Cli;

use clap::Parser;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_project_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "berth-main-test-dispatch-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    drop(fs::remove_dir_all(&root));
    fs::create_dir_all(&root).expect("create temporary project root");

    let config_path = root.join(".berth.yaml");
    crate::config::project_file::write_test_config_file(
        &config_path,
        "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
    )
    .unwrap_or_else(|error| {
        panic!(
            "write temporary config to {}: {error}",
            config_path.display()
        )
    });

    root
}

fn temporary_project_config() -> PathBuf {
    let root = temporary_project_root();
    let config_path = root.join(".berth.yaml");
    crate::config::project_file::write_test_config_file(
        &config_path,
        r#"
schema_version = 1
project_type = "project"
container_prefix = "acme"

[[service]]
preset = "laravel"
name = "app"
localhost_tls = true

[[service]]
preset = "mysql"
name = "db"
"#,
    )
    .expect("write project config");
    root
}

fn with_fake_curl<F, T>(output: &str, status_ok: bool, test: F) -> T
where
    F: FnOnce() -> T,
{
    let dir = std::env::temp_dir().join(format!(
        "berth-dispatch-open-curl-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&dir).expect("create temp curl dir");
    let script = dir.join("curl");
    let mut file = fs::File::create(&script).expect("create fake curl");

    if status_ok {
        writeln!(file, "#!/bin/sh\nprintf '%s' '{}'", output).expect("write fake curl");
    } else {
        writeln!(file, "#!/bin/sh\nexit 1").expect("write fake curl");
    }

    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script).expect("curl metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("set curl executable");
    }

    let command = script.to_string_lossy().to_string();
    let result = crate::cli::support::with_curl_command(&command, test);
    fs::remove_dir_all(&dir).ok();
    result
}

fn with_fake_open_command<F, T>(test: F) -> T
where
    F: FnOnce(&Path) -> T,
{
    let marker_dir = std::env::temp_dir().join(format!(
        "berth-dispatch-open-browser-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&marker_dir).expect("create marker dir");
    let marker = marker_dir.join("invoked");
    let command = marker_dir.join("open");
    let mut file = fs::File::create(&command).expect("create fake open command");
    writeln!(
        file,
        "#!/bin/sh\nprintf '%s\\n' \"$1\" > \"{}\"",
        marker.display()
    )
    .expect("write fake open");

    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&command).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&command, perms).expect("chmod");
    }

    let result = crate::cli::support::with_open_command(
        command.to_str().expect("open command path"),
        || {
            fs::remove_file(&marker).ok();
            test(&marker_dir)
        },
    );
    fs::remove_dir_all(&marker_dir).ok();
    result
}

#[test]
fn cli_dispatch_run_about_uses_temporary_project_root() {
    let project_root = temporary_project_root();
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "berth",
            "--project-root",
            project_root
                .to_str()
                .expect("project root path should be valid UTF-8"),
            "about",
        ]);

        assert!(crate::cli::dispatch::run(cli).is_ok());
    });
}

#[test]
fn cli_dispatch_run_status_succeeds_with_empty_services() {
    let project_root = temporary_project_root();
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "berth",
            "--project-root",
            project_root
                .to_str()
                .expect("project root path should be valid UTF-8"),
            "status",
        ]);

        assert!(crate::cli::dispatch::run(cli).is_ok());
    });
}

#[test]
fn cli_dispatch_runs_ls_with_json_format() {
    let project_root = temporary_project_root();
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "berth",
            "--project-root",
            project_root
                .to_str()
                .expect("project root path should be valid UTF-8"),
            "ls",
            "--format",
            "json",
        ]);

        assert!(crate::cli::dispatch::run(cli).is_ok());
    });
}

#[test]
fn cli_dispatch_generates_env_file_from_full_pipeline() {
    let project_root = temporary_project_config();
    let output_path = project_root.join(".env.generated");
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "berth",
            "--project-root",
            project_root
                .to_str()
                .expect("project root path should be valid UTF-8"),
            "env",
            "generate",
            "--output",
            output_path
                .to_str()
                .expect("output path should be valid UTF-8"),
        ]);

        assert!(crate::cli::dispatch::run(cli).is_ok());
        assert!(output_path.exists());
    });
}

#[test]
fn cli_dispatch_open_succeeds_and_records_browser_invocation() {
    let project_root = temporary_project_config();
    with_fake_open_command(|marker_dir| {
        with_fake_curl("200", true, || {
            crate::docker::with_test_runtime_lock(|| {
                let cli = Cli::parse_from([
                    "berth",
                    "--project-root",
                    project_root
                        .to_str()
                        .expect("project root path should be valid UTF-8"),
                    "open",
                ]);

                let result = crate::cli::dispatch::run(cli);
                assert!(result.is_ok(), "dispatch open failed: {result:?}");
                assert!(marker_dir.join("invoked").exists());
            });
        });
    });
}

#[test]
fn cli_dispatch_daemon_status_uses_explicit_path_without_local_project_context() {
    let project_root = temporary_project_root();
    crate::docker::with_test_runtime_lock(|| {
        let cli = Cli::parse_from([
            "berth",
            "daemon",
            "status",
            "--path",
            project_root
                .to_str()
                .expect("project root path should be valid UTF-8"),
        ]);

        assert!(crate::cli::dispatch::run(cli).is_ok());
    });
}
