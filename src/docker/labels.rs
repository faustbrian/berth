//! docker labels module.
//!
//! Contains Berth Docker label keys and values used by Berth command workflows.

use crate::config::{Kind, ServiceConfig};

pub(crate) const LABEL_MANAGED: &str = "dev.berth.managed";
pub(crate) const LABEL_SERVICE: &str = "dev.berth.service";
pub(crate) const LABEL_KIND: &str = "dev.berth.kind";
pub(crate) const LABEL_CONTAINER: &str = "dev.berth.container";
pub(crate) const LABEL_PROJECT: &str = "dev.berth.project";
pub(crate) const LABEL_INSTANCE: &str = "dev.berth.instance";
pub(crate) const LABEL_ROOT: &str = "dev.berth.root";
pub(crate) const LABEL_ROLE: &str = "dev.berth.role";
pub(crate) const LABEL_INSTALLATION: &str = "dev.berth.installation";
pub(crate) const VALUE_MANAGED_TRUE: &str = "true";

/// What a labeled container is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ContainerRole {
    /// A long-running project service (database, cache, app, worker).
    Service,
    /// A `--rm` container running one command for a service.
    OneOff,
    /// A named data volume of a dedicated service.
    Volume,
}

impl ContainerRole {
    const fn label_value(self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::OneOff => "one-off",
            Self::Volume => "volume",
        }
    }
}

/// Returns the `--label` arguments that prove berth owns a container, plus
/// the route labels the gateway derives its routes from.
///
/// Every container berth creates carries these labels so cleanup, reaping and
/// ownership checks can select berth's containers by label alone and never
/// touch containers berth did not create.
///
/// # Errors
///
/// Returns an error when the installation ID cannot be read or created.
pub(crate) fn ownership_label_args(
    service: &ServiceConfig,
    container_name: &str,
    role: ContainerRole,
) -> anyhow::Result<Vec<String>> {
    let mut labels = vec![
        format!("{LABEL_MANAGED}={VALUE_MANAGED_TRUE}"),
        format!("{LABEL_INSTALLATION}={}", installation_label_value()?),
        format!("{LABEL_SERVICE}={}", service.name),
        format!("{LABEL_KIND}={}", kind_label_value(service.kind)),
        format!("{LABEL_CONTAINER}={container_name}"),
        format!("{LABEL_ROLE}={}", role.label_value()),
    ];
    if let Some(project) = &service.runtime.project {
        labels.push(format!("{LABEL_PROJECT}={project}"));
    }
    if let Some(instance) = &service.runtime.instance {
        labels.push(format!("{LABEL_INSTANCE}={instance}"));
    }
    if let Some(root) = &service.runtime.root {
        labels.push(format!("{LABEL_ROOT}={}", root.display()));
    }
    if let Some(network) = &service.runtime.network {
        labels.push(format!("{}={network}", crate::gateway::NETWORK_LABEL));
    }
    let hosts = crate::gateway::route_hosts(service);
    if role == ContainerRole::Service && !hosts.is_empty() {
        labels.push(format!(
            "{}={}",
            crate::gateway::ROUTE_HOSTS_LABEL,
            hosts.join(",")
        ));
        labels.push(format!(
            "{}={}",
            crate::gateway::ROUTE_PORT_LABEL,
            service.resolved_container_port()
        ));
    }
    Ok(labels
        .into_iter()
        .flat_map(|label| ["--label".to_owned(), label])
        .collect())
}

/// The installation ID, or a placeholder in dry runs, which must not create
/// the berth home just to print a command.
fn installation_label_value() -> anyhow::Result<String> {
    if crate::docker::is_dry_run() {
        return Ok("<installation-id>".to_owned());
    }
    crate::home::installation_id()
}

pub(crate) fn kind_label_value(kind: Kind) -> &'static str {
    match kind {
        Kind::Database => "database",
        Kind::Cache => "cache",
        Kind::ObjectStore => "object-store",
        Kind::Search => "search",
        Kind::App => "app",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ContainerRole, ownership_label_args};
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service() -> ServiceConfig {
        let mut service: ServiceConfig = serde_json::from_value(serde_json::json!({
            "name": "db",
            "kind": "database",
            "driver": "mysql",
            "image": "mysql:8.4",
            "host": "127.0.0.1",
            "port": 3306
        }))
        .expect("service fixture");
        service.kind = Kind::Database;
        service.driver = Driver::Mysql;
        service
    }

    fn labels(args: &[String]) -> Vec<&str> {
        args.chunks(2)
            .map(|pair| {
                assert_eq!(pair.first().map(String::as_str), Some("--label"));
                pair.get(1).map(String::as_str).unwrap_or_default()
            })
            .collect()
    }

    #[test]
    fn instance_labels_identify_project_instance_and_root() {
        let mut service = service();
        service.runtime.project = Some("shipit-api".to_owned());
        service.runtime.instance = Some("agent-1-97fe08".to_owned());
        service.runtime.root = Some(PathBuf::from("/work/agent-1"));

        let args = ownership_label_args(
            &service,
            "berth-shipit-api-agent-1-97fe08-db",
            ContainerRole::Service,
        )
        .expect("labels");
        let labels = labels(&args);
        for expected in [
            "dev.berth.managed=true",
            "dev.berth.service=db",
            "dev.berth.role=service",
            "dev.berth.project=shipit-api",
            "dev.berth.instance=agent-1-97fe08",
            "dev.berth.root=/work/agent-1",
            "dev.berth.container=berth-shipit-api-agent-1-97fe08-db",
            "dev.berth.installation=00000000000000000000000000000000",
        ] {
            assert!(labels.contains(&expected), "missing {expected}: {labels:?}");
        }
    }

    #[test]
    fn services_without_runtime_identity_get_only_ownership_labels() {
        let args = ownership_label_args(&service(), "db", ContainerRole::OneOff).expect("labels");
        let labels = labels(&args);
        assert!(labels.contains(&"dev.berth.role=one-off"));
        assert!(
            !labels
                .iter()
                .any(|label| label.starts_with("dev.berth.instance="))
        );
    }

    #[test]
    fn routed_services_carry_their_hosts_port_and_network() {
        let mut service = service();
        service.runtime.network = Some("berth_p_a".to_owned());
        service.resolved_domain = Some("a-p-db.berth.localhost".to_owned());
        service.runtime.gateway_https_port = Some(8443);
        let args = ownership_label_args(&service, "c", ContainerRole::Service).expect("labels");
        let labels = labels(&args);
        assert!(
            labels.contains(&"dev.berth.network=berth_p_a"),
            "{labels:?}"
        );
        assert!(
            labels.contains(&"dev.berth.route.hosts=a-p-db.berth.localhost"),
            "{labels:?}"
        );
        assert!(labels.contains(&"dev.berth.route.port=3306"), "{labels:?}");

        let one_off = ownership_label_args(&service, "c", ContainerRole::OneOff).expect("labels");
        assert!(
            !labels_of(&one_off)
                .iter()
                .any(|label| label.starts_with("dev.berth.route.")),
            "one-off containers never receive routes"
        );
    }

    fn labels_of(args: &[String]) -> Vec<String> {
        labels(args).into_iter().map(ToOwned::to_owned).collect()
    }

    #[test]
    fn direct_localhost_tls_services_carry_no_route_labels() {
        let mut service = service();
        service.resolved_domain = Some("a-p-db.berth.localhost".to_owned());
        service.runtime.gateway_https_port = Some(8443);
        service.localhost_tls = true;
        let args = ownership_label_args(&service, "c", ContainerRole::Service).expect("labels");
        assert!(
            !labels_of(&args)
                .iter()
                .any(|label| label.starts_with("dev.berth.route.")),
            "test-runtime containers must never be routed"
        );
    }
}
