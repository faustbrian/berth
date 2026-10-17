//! One-off container execution for non-running serve targets.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

use super::super::{resolve_runtime_image, resolve_volume_mapping};

/// Runs a command in a temporary container derived from target runtime settings.
///
/// This mirrors target volume/env wiring so commands behave like in the regular
/// app container.
pub(super) fn run_command_one_off(
    target: &ServiceConfig,
    command: &[String],
    tty: bool,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    let runtime_image = resolve_runtime_image(target, true, injected_env, project_root)?;
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    args.extend(crate::docker::ownership_label_args(
        target,
        &format!("{}-one-off", target.container_name()?),
        crate::docker::ContainerRole::OneOff,
    ));
    args.push(if tty { "-it" } else { "-i" }.to_owned());
    args.push("-w".to_owned());
    args.push("/app".to_owned());

    if let Some(volumes) = &target.volumes {
        for volume in volumes {
            args.push("-v".to_owned());
            args.push(resolve_volume_mapping(volume, project_root)?);
        }
    }

    for (key, value) in injected_env {
        args.push("-e".to_owned());
        args.push(format!("{key}={value}"));
    }

    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
    }

    args.push(runtime_image);
    args.extend(command.iter().cloned());

    super::docker_cmd::run_or_log_docker(
        &target.name,
        &args,
        "failed to execute one-off app command",
        |_| "one-off app command failed".to_owned(),
    )
}
