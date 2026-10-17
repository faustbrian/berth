//! config runtime env module.
//!
//! Contains config runtime env logic used by Berth command workflows.

use anyhow::{Context, Result};

use super::Config;
use naming::{is_default_runtime_env, normalize_runtime_env_name};
use ports::{runtime_env_port_offset, shift_port};

mod naming;
mod ports;

pub(super) fn apply_runtime_env(config: &mut Config, env_name: &str) -> Result<()> {
    let normalized = normalize_runtime_env_name(env_name)?;
    if is_default_runtime_env(&normalized) {
        return Ok(());
    }

    let suffix = format!("-{normalized}");
    let port_offset = runtime_env_port_offset(&normalized);

    for service in &mut config.service {
        let base_name = service.container_name().with_context(|| {
            format!(
                "failed to resolve base container name for '{}'",
                service.name
            )
        })?;
        service.resolved_container_name = Some(format!("{base_name}{suffix}"));
        // A runtime environment gets its own network so its services never
        // share DNS aliases with the instance's default services.
        if let Some(network) = &mut service.runtime.network {
            network.push_str(&suffix);
        }
        // Runtime environments keep the 7.0.0 model: direct, port-offset
        // local URLs and no gateway routes. Routes belong to the instance's
        // default environment; a runtime environment's name (for example the
        // `testing-pool-{hash}-{slot}` test leases) would not fit a 63-byte
        // DNS label and its authored domains would collide with the default.
        service.resolved_domain = None;
        service.domain = None;
        service.domains = None;
        service.runtime.gateway_https_port = None;
        service.port = shift_port(service.port, port_offset, &service.name, "port")?;
        if let Some(smtp_port) = service.smtp_port {
            service.smtp_port = Some(shift_port(
                smtp_port,
                port_offset,
                &service.name,
                "smtp_port",
            )?);
        }
    }

    Ok(())
}

/// Returns config override base name for an optional runtime environment.
pub(super) fn runtime_env_config_file_name(runtime_env: Option<&str>) -> Result<Option<String>> {
    let Some(env_name) = runtime_env else {
        return Ok(None);
    };

    let normalized = normalize_runtime_env_name(env_name)?;
    Ok(Some(format!(".berth.{normalized}")))
}

/// Returns the default value for env file name.
pub(super) fn default_env_file_name(runtime_env: Option<&str>) -> Result<String> {
    let Some(env_name) = runtime_env else {
        return Ok(".env".to_owned());
    };

    let normalized = normalize_runtime_env_name(env_name)?;
    if is_default_runtime_env(&normalized) {
        return Ok(".env".to_owned());
    }

    if normalized == "test" || normalized == "testing" || normalized.starts_with("testing-") {
        return Ok(".env.testing".to_owned());
    }

    Ok(format!(".env.{normalized}"))
}
