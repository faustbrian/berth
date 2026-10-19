//! config expansion service module.
//!
//! Contains config expansion service logic used by Berth command workflows.

use anyhow::Result;

use super::super::{RawServiceConfig, ServiceConfig, presets};
use conflict::validate_preset_conflicts;
use fields::{merge_opt_copy, merge_opt_owned, pick_required, value_or_default};
use hooks::expand_hooks;
use selection::resolve_name_and_image;

mod conflict;
mod fields;
mod hooks;
mod selection;

pub(super) fn expand_raw_service(raw: RawServiceConfig) -> Result<ServiceConfig> {
    let preset = raw.preset.as_deref();
    let defaults = match raw.preset.as_deref() {
        Some(preset) => Some(presets::preset_defaults(preset)?),
        None => None,
    };

    let kind = value_or_default(raw.kind, defaults.as_ref().map(|d| d.kind), "kind")?;
    let driver = value_or_default(raw.driver, defaults.as_ref().map(|d| d.driver), "driver")?;

    validate_preset_conflicts(
        &raw,
        defaults.as_ref().map(|d| d.kind),
        defaults.as_ref().map(|d| d.driver),
        kind,
        driver,
    )?;

    let (name, image) = resolve_name_and_image(&raw, defaults.as_ref(), driver)?;
    let authored_credentials =
        raw.database.is_some() || raw.username.is_some() || raw.password.is_some();
    let authored_container_fields: Vec<&'static str> = [
        ("volumes", raw.volumes.is_some()),
        ("env", raw.env.is_some()),
        ("port", raw.port.is_some()),
        ("container_port", raw.container_port.is_some()),
        ("restart", raw.restart.is_some()),
        ("hook", !raw.hook.is_empty()),
    ]
    .into_iter()
    .filter_map(|(field, set)| set.then_some(field))
    .collect();

    let mut env = raw.env;
    if let Some(forced_env) = defaults.as_ref().and_then(|d| d.forced_env.as_ref()) {
        let values = env.get_or_insert_with(Default::default);
        for (key, value) in forced_env {
            values.insert((*key).to_owned(), (*value).to_owned());
        }
    }

    Ok(ServiceConfig {
        name,
        kind,
        driver,
        image,
        host: pick_required(
            raw.host,
            defaults.as_ref().map(|d| d.host),
            "127.0.0.1".to_owned(),
        ),
        port: merge_opt_copy(raw.port, defaults.as_ref().and_then(|d| d.port)).unwrap_or(0),
        database: merge_opt_owned(raw.database, defaults.as_ref().and_then(|d| d.database)),
        username: merge_opt_owned(raw.username, defaults.as_ref().and_then(|d| d.username)),
        password: merge_opt_owned(raw.password, defaults.as_ref().and_then(|d| d.password)),
        bucket: merge_opt_owned(raw.bucket, defaults.as_ref().and_then(|d| d.bucket)),
        access_key: merge_opt_owned(raw.access_key, defaults.as_ref().and_then(|d| d.access_key)),
        secret_key: merge_opt_owned(raw.secret_key, defaults.as_ref().and_then(|d| d.secret_key)),
        api_key: merge_opt_owned(raw.api_key, defaults.as_ref().and_then(|d| d.api_key)),
        region: merge_opt_owned(raw.region, defaults.as_ref().and_then(|d| d.region)),
        scheme: merge_opt_owned(raw.scheme, defaults.as_ref().and_then(|d| d.scheme)),
        domain: raw.domain,
        domains: raw.domains,
        routes: raw.routes.map(|routes| routes.into_iter().collect()),
        resolved_domain: None,
        container_port: merge_opt_copy(
            raw.container_port,
            defaults.as_ref().and_then(|d| d.container_port),
        ),
        smtp_port: merge_opt_copy(raw.smtp_port, defaults.as_ref().and_then(|d| d.smtp_port)),
        volumes: raw
            .volumes
            .or_else(|| defaults.as_ref().and_then(|d| d.volumes.clone())),
        env,
        command: raw
            .command
            .or_else(|| defaults.as_ref().and_then(|d| d.command.clone())),
        depends_on: raw.depends_on,
        seed_file: raw.seed_file,
        hook: expand_hooks(raw.hook)?,
        health_path: raw.health_path.or_else(|| {
            preset
                .and_then(presets::default_health_path_for_preset)
                .map(str::to_owned)
        }),
        health_statuses: raw
            .health_statuses
            .or_else(|| preset.and_then(presets::default_health_statuses_for_preset)),
        restart: raw.restart,
        localhost_tls: raw.localhost_tls.unwrap_or(false),
        octane: raw
            .octane
            .unwrap_or_else(|| defaults.as_ref().is_some_and(|d| d.octane)),
        octane_workers: merge_opt_copy(
            raw.octane_workers,
            defaults.as_ref().and_then(|d| d.octane_workers),
        ),
        octane_max_requests: merge_opt_copy(
            raw.octane_max_requests,
            defaults.as_ref().and_then(|d| d.octane_max_requests),
        ),
        php_extensions: raw
            .php_extensions
            .or_else(|| defaults.as_ref().and_then(|d| d.php_extensions.clone())),
        trust_container_ca: raw
            .trust_container_ca
            .unwrap_or_else(|| defaults.as_ref().is_some_and(|d| d.trust_container_ca)),
        env_mapping: raw.env_mapping,
        javascript: raw.javascript,
        container_name: raw.container_name,
        resolved_container_name: None,
        runtime: crate::config::types::ServiceRuntime {
            publish_host_port: raw.publish.unwrap_or(false),
            isolation: raw.isolation,
            authored_credentials,
            authored_container_fields,
            ..Default::default()
        },
    })
}
