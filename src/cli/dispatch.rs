//! Top-level CLI dispatch pipeline.
//!
//! This is the single entrypoint that applies global CLI process settings
//! (`--no-color`, `--quiet`, `--dry-run`), runs setup-only commands that do not
//! require config loading, and then routes to primary/secondary command trees.

use anyhow::Result;

use super::args::{Cli, Commands};
use super::handlers;
use crate::config;
use crate::docker;
use crate::output;

mod bootstrap;
mod context;
mod lease;
mod primary;
mod secondary;

/// Executes one full CLI invocation.
///
/// Dispatch order matters:
/// 1. Apply global process-level flags.
/// 2. Handle setup commands that can run without config.
/// 3. Load and optionally runtime-patch config.
/// 4. Attempt primary dispatch, then fall through to secondary.
pub(crate) fn run(mut cli: Cli) -> Result<()> {
    apply_json_flags(&mut cli);
    if cli.no_color {
        colored::control::set_override(false);
    }

    apply_runtime_policy_overrides(&cli);
    output::init(cli.quiet);
    docker::set_dry_run(cli.dry_run);
    let dispatch_context = context::CliDispatchContext::from_cli(&cli);

    if bootstrap::handle_setup_commands(&cli, &dispatch_context)? {
        return Ok(());
    }

    // Commands about every instance run outside any project.
    match &cli.command {
        Commands::Ls(args) if !args.services => {
            docker::set_container_engine(cli.engine.unwrap_or_default());
            lease::reap_in_passing(None);
            return handlers::handle_list_instances(&args.format, args.json);
        }
        Commands::Gc(args) => {
            docker::set_container_engine(cli.engine.unwrap_or_default());
            return handlers::handle_gc(args.json);
        }
        _ => {}
    }

    let configured_engine = config::load_container_engine_with(
        config::LoadConfigPathOptions::new(
            dispatch_context.config_path(),
            dispatch_context.project_root(),
        )
        .with_runtime_env(dispatch_context.runtime_env()),
    )?;
    docker::set_container_engine(cli.engine.or(configured_engine).unwrap_or_default());

    let mut config = bootstrap::load_config_for_cli(&dispatch_context)?;
    let lease = match lease::renew_and_reap(&cli, &config) {
        Ok(lease) => lease,
        Err(error) => {
            // `up --json` promises one document on stdout, failures included
            // (a bad `--ttl`, an `--owner-pid` that does not run).
            if let Commands::Up(args) = &cli.command
                && args.json
            {
                lease::print_failure_document(&config, &error)?;
            }
            return Err(error);
        }
    };
    let result = match primary::dispatch_primary(&cli, &mut config, &dispatch_context) {
        Some(result) => result,
        None => secondary::dispatch_secondary(&cli, &mut config, &dispatch_context),
    };
    match lease {
        Some(lease) => result.and(lease.finish()),
        None => result,
    }
}

/// `--json` on the inspection commands means `--format json`. Every
/// machine-readable mode reserves stdout for its output, so image pulls and
/// builds a repair runs (`doctor --fix`) go to stderr.
fn apply_json_flags(cli: &mut Cli) {
    let machine_readable = match &cli.command {
        Commands::Ps(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        Commands::Url(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        Commands::About(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        Commands::Health(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        Commands::Doctor(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        Commands::Logs(args) => args.json,
        Commands::Ls(args) => args.json || args.format.eq_ignore_ascii_case("json"),
        _ => false,
    };
    if machine_readable {
        output::reserve_stdout();
    }
    normalize_format(cli);
}

/// `--json` sets `--format json`; any other value is lowercased.
fn normalize_format(cli: &mut Cli) {
    // clap accepts `--format JSON` (ignore_case) but keeps the spelling;
    // handlers compare against the lowercase name, so normalize it once.
    let (format, json) = match &mut cli.command {
        Commands::Ps(args) => (&mut args.format, args.json),
        Commands::Url(args) => (&mut args.format, args.json),
        Commands::About(args) => (&mut args.format, args.json),
        Commands::Health(args) => (&mut args.format, args.json),
        Commands::Doctor(args) => (&mut args.format, args.json),
        Commands::Ls(args) => (&mut args.format, args.json),
        _ => return,
    };
    let normalized = if json {
        "json".to_owned()
    } else {
        format.to_ascii_lowercase()
    };
    *format = normalized;
}

fn apply_runtime_policy_overrides(cli: &Cli) {
    docker::set_policy_overrides(docker::DockerPolicyOverrides {
        max_heavy_ops: cli.docker_max_heavy_ops,
        max_build_ops: cli.docker_max_build_ops,
        retry_budget: cli.docker_retry_budget,
    });
    handlers::set_testing_runtime_pool_size_override(cli.test_runtime_pool_size);
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli::args::Cli;
    use clap::Parser;

    fn minimal_config_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "berth-dispatch-run-{}-{}",
            std::process::id(),
            nanos
        ));
        drop(fs::remove_dir_all(&dir));
        fs::create_dir_all(&dir).expect("create temporary project root");
        let config_path = dir.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(
            &config_path,
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write minimal config");
        dir
    }

    #[test]
    fn run_dispatches_about_from_full_pipeline() {
        let project_root = minimal_config_dir();
        crate::docker::with_test_runtime_lock(|| {
            let args = [
                "berth",
                "--project-root",
                project_root.to_str().expect("project root is valid utf-8"),
                "about",
            ];
            let cli = Cli::try_parse_from(args).expect("parse cli");
            let result = super::run(cli);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn run_dispatches_secondary_status_via_full_pipeline() {
        let project_root = minimal_config_dir();
        crate::docker::with_test_runtime_lock(|| {
            let args = [
                "berth",
                "--project-root",
                project_root.to_str().expect("project root is valid utf-8"),
                "status",
            ];
            let cli = Cli::parse_from(args);
            let result = super::run(cli);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn run_applies_container_engine_from_config() {
        let project_root = minimal_config_dir();
        let config_path = project_root.join(".berth.yaml");
        crate::config::project_file::write_test_config_file(&config_path,
            "schema_version = 1\nproject_type = \"project\"\ncontainer_engine = \"podman\"\nservice = []\nswarm = []\n",
        )
        .expect("write podman config");

        crate::docker::with_container_engine(crate::config::ContainerEngine::Docker, || {
            let args = [
                "berth",
                "--project-root",
                project_root.to_str().expect("project root is valid utf-8"),
                "status",
            ];
            let cli = Cli::parse_from(args);
            let result = super::run(cli);
            assert!(result.is_ok());
            assert_eq!(
                crate::docker::container_engine(),
                crate::config::ContainerEngine::Podman
            );
        });
    }
}

#[cfg(test)]
mod format_tests {
    use super::normalize_format;
    use crate::cli::args::{Cli, Commands};
    use clap::Parser;

    fn format_of(args: &[&str]) -> String {
        let mut cli = Cli::parse_from(args);
        normalize_format(&mut cli);
        match cli.command {
            Commands::Ps(args) => args.format,
            Commands::Url(args) => args.format,
            Commands::Ls(args) => args.format,
            Commands::Doctor(args) => args.format,
            _ => unreachable!("only inspection commands are parsed here"),
        }
    }

    #[test]
    fn any_spelling_of_json_reaches_handlers_as_json() {
        assert_eq!(format_of(&["berth", "ps", "--format", "JSON"]), "json");
        assert_eq!(format_of(&["berth", "url", "--format", "Json"]), "json");
        assert_eq!(format_of(&["berth", "ls", "--format", "JSON"]), "json");
        assert_eq!(format_of(&["berth", "doctor", "--json"]), "json");
        assert_eq!(format_of(&["berth", "ps", "--format", "TABLE"]), "table");
    }
}
