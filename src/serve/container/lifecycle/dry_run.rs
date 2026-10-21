//! Dry-run output helpers for serve container lifecycle commands.

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

/// Prints the `docker rm`/`docker run` commands that would be executed.
pub(super) fn print_dry_run_container_start(
    target: &ServiceConfig,
    container_name: &str,
    recreate: bool,
    runtime_image: &str,
    smtp_port: Option<u16>,
) -> anyhow::Result<()> {
    if recreate {
        let remove_args = vec!["rm".to_owned(), "-f".to_owned(), container_name.to_owned()];
        output::event(
            &target.name,
            LogLevel::Info,
            &format!(
                "[dry-run] {}",
                crate::docker::runtime_command_text(&remove_args)
            ),
            Persistence::Transient,
        );
    }

    let mut mapping = format!(
        "-p {}:{}:{}",
        target.host,
        target.port,
        target.resolved_container_port()
    );
    if let Some(smtp_port) = smtp_port {
        mapping.push_str(&format!(" -p {}:{}:1025", target.host, smtp_port));
    }

    let mut run_args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        container_name.to_owned(),
    ];
    run_args.extend(crate::docker::ownership_label_args(
        target,
        container_name,
        crate::docker::ContainerRole::Service,
    )?);
    run_args.extend(crate::network::network_run_args(target, true));
    run_args.extend(crate::docker::resource_limit_args(target));
    if crate::network::publishes_host_port(target) {
        run_args.push(mapping);
    }
    run_args.push(runtime_image.to_owned());
    output::event(
        &target.name,
        LogLevel::Info,
        &format!(
            "[dry-run] {}",
            crate::docker::runtime_command_text(&run_args)
        ),
        Persistence::Transient,
    );
    Ok(())
}
