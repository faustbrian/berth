//! `docker run` arguments that attach a container to its instance network.

use crate::config::ServiceConfig;

/// Returns `--network` (and, for long-running services, `--network-alias`)
/// arguments, or nothing for services without an instance network.
#[must_use]
pub(crate) fn network_run_args(service: &ServiceConfig, with_alias: bool) -> Vec<String> {
    let Some(network) = &service.runtime.network else {
        return Vec::new();
    };
    let mut args = vec!["--network".to_owned(), network.clone()];
    if with_alias {
        args.push("--network-alias".to_owned());
        args.push(service.name.clone());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::network_run_args;
    use crate::config::ServiceConfig;

    fn service() -> ServiceConfig {
        serde_json::from_value(serde_json::json!({
            "name": "db", "kind": "database", "driver": "postgres",
            "image": "postgres:17", "host": "127.0.0.1", "port": 5432
        }))
        .expect("service fixture")
    }

    #[test]
    fn services_join_their_instance_network_under_their_name() {
        let mut service = service();
        assert!(
            network_run_args(&service, true).is_empty(),
            "no network, no args"
        );
        service.runtime.network = Some("berth_api_a-1".to_owned());
        assert_eq!(
            network_run_args(&service, true),
            ["--network", "berth_api_a-1", "--network-alias", "db"]
        );
        assert_eq!(
            network_run_args(&service, false),
            ["--network", "berth_api_a-1"]
        );
    }
}
