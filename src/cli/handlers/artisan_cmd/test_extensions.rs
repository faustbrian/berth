//! Test-runtime PHP extension adjustments for artisan test workflows.

use crate::config::ServiceConfig;

const PCOV_EXTENSION: &str = "pcov";

/// Ensures artisan test runtime requirements are present on the app target.
pub(super) fn ensure_artisan_test_runtime_extensions(target: &mut ServiceConfig) {
    let extensions = target.php_extensions.get_or_insert_with(Vec::new);
    if extensions
        .iter()
        .any(|extension| extension.eq_ignore_ascii_case(PCOV_EXTENSION))
    {
        return;
    }

    extensions.push(PCOV_EXTENSION.to_owned());
}

#[cfg(test)]
mod tests {
    use super::ensure_artisan_test_runtime_extensions;
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8080,
            database: None,
            username: None,
            password: None,
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
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    #[test]
    fn ensure_artisan_test_runtime_extensions_adds_pcov_when_missing() {
        let mut target = service();

        ensure_artisan_test_runtime_extensions(&mut target);

        assert_eq!(target.php_extensions, Some(vec!["pcov".to_owned()]));
    }

    #[test]
    fn ensure_artisan_test_runtime_extensions_preserves_existing_extensions() {
        let mut target = service();
        target.php_extensions = Some(vec!["intl".to_owned()]);

        ensure_artisan_test_runtime_extensions(&mut target);

        assert_eq!(
            target.php_extensions,
            Some(vec!["intl".to_owned(), "pcov".to_owned()])
        );
    }

    #[test]
    fn ensure_artisan_test_runtime_extensions_avoids_duplicate_pcov() {
        let mut target = service();
        target.php_extensions = Some(vec!["pcov".to_owned(), "intl".to_owned()]);

        ensure_artisan_test_runtime_extensions(&mut target);

        assert_eq!(
            target.php_extensions,
            Some(vec!["pcov".to_owned(), "intl".to_owned()])
        );
    }
}
