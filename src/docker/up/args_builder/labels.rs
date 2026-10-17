//! docker up args builder labels module.
//!
//! Contains docker run label arguments used by Berth command workflows.

use crate::config::ServiceConfig;
use crate::docker::{
    LABEL_CONTAINER, LABEL_KIND, LABEL_MANAGED, LABEL_SERVICE, VALUE_MANAGED_TRUE, kind_label_value,
};

pub(super) fn append_labels(args: &mut Vec<String>, service: &ServiceConfig, container_name: &str) {
    args.push("--label".to_owned());
    args.push(format!("{LABEL_MANAGED}={VALUE_MANAGED_TRUE}"));

    args.push("--label".to_owned());
    args.push(format!("{LABEL_SERVICE}={}", service.name));

    args.push("--label".to_owned());
    args.push(format!("{LABEL_KIND}={}", kind_label_value(service.kind)));

    args.push("--label".to_owned());
    args.push(format!("{LABEL_CONTAINER}={container_name}"));
}

#[cfg(test)]
mod tests {
    use super::append_labels;
    use crate::config::{Driver, Kind, ServiceConfig};

    #[test]
    fn appends_berth_labels_for_container_ownership() {
        let service = ServiceConfig {
            name: "db".to_owned(),
            kind: Kind::Database,
            driver: Driver::Mysql,
            image: "mysql:8.0".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 3306,
            database: Some("app".to_owned()),
            username: Some("root".to_owned()),
            password: Some("secret".to_owned()),
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: None,
            domains: None,
            resolved_domain: None,
            container_port: None,
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            restart: None,
            localhost_tls: false,
            octane: false,
            octane_workers: None,
            octane_max_requests: None,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            javascript: None,
            container_name: None,
            resolved_container_name: Some("acme-db".to_owned()),
        };
        let mut args = Vec::new();

        append_labels(&mut args, &service, "acme-db");

        let rendered = args.join(" ");
        assert!(rendered.contains("--label dev.berth.managed=true"));
        assert!(rendered.contains("--label dev.berth.service=db"));
        assert!(rendered.contains("--label dev.berth.kind=database"));
        assert!(rendered.contains("--label dev.berth.container=acme-db"));
    }
}
