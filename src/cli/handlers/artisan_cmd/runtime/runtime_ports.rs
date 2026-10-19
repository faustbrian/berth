//! testing runtime port and env preparation helpers.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::{cli, config, env};

pub(super) fn prepare_testing_runtime(
    config: &mut config::Config,
) -> Result<(HashSet<String>, HashMap<String, String>)> {
    let remapped_services = assign_testing_runtime_ports(config)?;
    let mut app_env = env::inferred_app_env(config);
    // Test runs force `APP_ENV=testing` (the test command passes it too);
    // Laravel then loads `.env.testing`, which decides whether the managed
    // key is needed. An explicit `APP_ENV` in the app's `env` still wins in
    // the container, but the test command's `--env=testing` selects the file.
    app_env.insert("APP_ENV".to_owned(), "testing".to_owned());
    env::apply_managed_runtime_env(config, &mut app_env, Some("testing"))?;

    Ok((remapped_services, app_env))
}

pub(super) fn assign_testing_runtime_ports(config: &mut config::Config) -> Result<HashSet<String>> {
    let mut remapped = HashSet::new();
    let mut used_ports = cli::support::collect_service_host_ports(&config.service);

    for service in &mut config.service {
        if service.kind == config::Kind::App {
            service.localhost_tls = true;
            service.domain = None;
            service.domains = None;
        }
        // Test leases keep 7.0.0's random published ports; a shared tenant
        // has none.
        if service.runtime.shared.is_some() {
            continue;
        }
        service.runtime.publish_host_port = true;

        let remap = cli::support::remap_random_ports(service, &mut used_ports, true)?;
        if remap.was_remapped() {
            remapped.insert(service.name.clone());
        }
    }

    Ok(remapped)
}
