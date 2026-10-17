//! Docker run argument primitives for serve lifecycle.

use anyhow::Result;
use std::path::Path;

use crate::config::ServiceConfig;

pub(super) fn build_base_run_args(target: &ServiceConfig) -> Result<Vec<String>> {
    let container_name = target.container_name()?;
    let mut args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        container_name.clone(),
    ];
    args.extend(crate::docker::ownership_label_args(
        target,
        &container_name,
        crate::docker::ContainerRole::Service,
    )?);
    args.extend(crate::network::network_run_args(target, true));
    if crate::network::publishes_host_port(target) {
        args.extend([
            "-p".to_owned(),
            format!(
                "{}:{}:{}",
                target.host,
                target.port,
                target.resolved_container_port()
            ),
        ]);
    }
    Ok(args)
}

pub(super) fn append_smtp_port_mapping(run_args: &mut Vec<String>, host: &str, smtp_port: u16) {
    run_args.push("-p".to_owned());
    run_args.push(format!("{host}:{smtp_port}:1025"));
}

pub(super) fn append_volume_args<F>(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    project_root: &Path,
    mut resolve_volume_mapping: F,
) -> Result<()>
where
    F: FnMut(&str, &Path) -> Result<String>,
{
    if let Some(volumes) = &target.volumes {
        for volume in volumes {
            run_args.push("-v".to_owned());
            run_args.push(resolve_volume_mapping(volume, project_root)?);
        }
    }

    Ok(())
}

pub(super) fn append_runtime_image_and_command<F>(
    run_args: &mut Vec<String>,
    runtime_image: &str,
    target: &ServiceConfig,
    mut resolved_run_command: F,
) where
    F: FnMut(&ServiceConfig) -> Option<Vec<String>>,
{
    run_args.push(runtime_image.to_owned());

    if let Some(command) = resolved_run_command(target) {
        run_args.extend(command);
    }
}
