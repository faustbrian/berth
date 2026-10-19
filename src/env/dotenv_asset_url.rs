//! Refuses a dotenv `ASSET_URL` on an app whose sites have their own routes.

use anyhow::Result;
use std::collections::HashMap;

use crate::config::Config;

/// Fails when a Laravel app served on extra `routes` loads a dotenv that sets
/// `ASSET_URL`.
///
/// berth injects no `ASSET_URL` for such an app, so `asset()` follows the
/// host each site is served on. Laravel's environment is immutable, so a
/// dotenv `ASSET_URL` would then apply on every host, and the extension
/// sites would load their module scripts cross-origin, which browsers refuse
/// without CORS headers. Without gateway routes (`--env` namespaces and test
/// leases) the sites share one host, so the dotenv value is left alone.
///
/// # Errors
///
/// Returns an error naming the service and the dotenv file.
pub(crate) fn ensure_no_dotenv_asset_url(
    config: &Config,
    vars: &HashMap<String, String>,
) -> Result<()> {
    for service in &config.service {
        if service.routes.is_none()
            || !crate::gateway::has_gateway_routes(service)
            || !super::is_laravel_app(service)
        {
            continue;
        }
        let Some(root) = service.runtime.root.as_deref() else {
            continue;
        };
        let app_env = super::dotenv_file::container_app_env(service, vars);
        let dotenv = super::dotenv_file::loaded_dotenv(root, app_env);
        if super::dotenv_file::dotenv_sets(&dotenv, "ASSET_URL") {
            anyhow::bail!(
                "service '{}' has `routes`, but {} sets ASSET_URL, which would load every \
                 site's assets from one origin; remove it so each site loads its own",
                service.name,
                dotenv.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_no_dotenv_asset_url;
    use crate::config::{Config, ServiceConfig};
    use std::collections::{BTreeMap, HashMap};

    fn root(label: &str) -> std::path::PathBuf {
        let root = crate::test_support::temp_root()
            .join(format!("berth-asset-url-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).expect("root");
        std::fs::write(root.join("artisan"), "#!/usr/bin/env php\n").expect("artisan");
        root
    }

    fn service(name: &str, driver: &str, root: &std::path::Path) -> ServiceConfig {
        let mut service: ServiceConfig = serde_json::from_value(serde_json::json!({
            "name": name, "kind": "app", "driver": driver,
            "image": "image", "host": "127.0.0.1", "port": 8000
        }))
        .expect("service");
        service.runtime.root = Some(root.to_path_buf());
        service.resolved_domain = Some(format!("a-1-api-{name}.berth.localhost"));
        service.runtime.gateway_https_port = Some(8443);
        service
    }

    /// A mailpit listed first, then the routed Laravel web app.
    fn project(root: &std::path::Path) -> Config {
        let mut web = service("web", "frankenphp", root);
        web.routes = Some(BTreeMap::from([("ee".to_owned(), "APP_EE_URL".to_owned())]));
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("api".to_owned()),
            domain_strategy: None,
            service: vec![service("mail", "mailhog", root), web],
            swarm: vec![],
        }
    }

    fn check(config: &Config, app_env: Option<&str>) -> Result<(), String> {
        let vars: HashMap<String, String> = app_env
            .map(|env| HashMap::from([("APP_ENV".to_owned(), env.to_owned())]))
            .unwrap_or_default();
        ensure_no_dotenv_asset_url(config, &vars).map_err(|error| format!("{error:#}"))
    }

    #[test]
    fn a_routed_app_whose_dotenv_sets_asset_url_is_refused_naming_it() {
        let root = root("refused");
        let config = project(&root);
        for line in [
            "ASSET_URL=https://cdn.test\n",
            "export ASSET_URL=https://cdn.test\n",
            "ASSET_URL = https://cdn.test\n",
            "ASSET_URL=\"https://cdn.test\"\n",
            "  export   ASSET_URL='https://cdn.test' # mine\n",
        ] {
            std::fs::write(root.join(".env"), line).expect("env");
            let error = check(&config, None).expect_err(line);
            assert!(
                error.contains("service 'web'")
                    && error.contains(".env")
                    && error.contains("ASSET_URL"),
                "{line:?}: {error}"
            );
            assert!(!error.contains("'mail'"), "{error}");
        }
        for line in [
            "# ASSET_URL=https://cdn.test\n",
            "VITE_ASSET_URL=https://cdn.test\n",
            "ASSET_URL_CDN=https://cdn.test\n",
            "ASSET_URL=\n",
            "ASSET_URL=\"\"\n",
            "ASSET_URL=null\n",
            "ASSET_URL=(null)\n",
            "ASSET_URL=empty\n",
            "ASSET_URL=(empty)\n",
            "ASSET_URL=(false)\n",
            "ASSET_URL=https://cdn.test\nASSET_URL=\n",
        ] {
            std::fs::write(root.join(".env"), line).expect("env");
            assert_eq!(check(&config, None), Ok(()), "{line:?}");
        }
        std::fs::remove_file(root.join(".env")).expect("rm");
        assert_eq!(check(&config, None), Ok(()), "no dotenv");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn the_dotenv_laravel_loads_for_the_app_env_is_the_one_checked() {
        let root = root("app-env");
        let config = project(&root);
        std::fs::write(root.join(".env.staging"), "ASSET_URL=https://cdn.test\n").expect("env");
        let error = check(&config, Some("staging")).expect_err("staging dotenv");
        assert!(error.contains(".env.staging"), "{error}");

        std::fs::write(root.join(".env.staging"), "APP_DEBUG=true\n").expect("env");
        std::fs::write(root.join(".env"), "ASSET_URL=https://cdn.test\n").expect("env");
        assert_eq!(
            check(&config, Some("staging")),
            Ok(()),
            ".env is not loaded"
        );

        std::fs::remove_file(root.join(".env.staging")).expect("rm");
        let error = check(&config, Some("staging")).expect_err("falls back to .env");
        assert!(
            error.contains(".env") && !error.contains(".env.staging"),
            "{error}"
        );

        // The preset sets `APP_ENV` in the app's own env, which the
        // container receives.
        let mut own = project(&root);
        std::fs::write(root.join(".env.local"), "ASSET_URL=https://cdn.test\n").expect("env");
        std::fs::write(root.join(".env"), "APP_DEBUG=true\n").expect("env");
        own.service[1].env = Some(HashMap::from([("APP_ENV".to_owned(), "local".to_owned())]));
        let error = check(&own, None).expect_err("own APP_ENV");
        assert!(error.contains(".env.local"), "{error}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn the_apps_own_app_env_wins_over_an_injected_one_as_in_its_container() {
        let root = root("app-env-precedence");
        let mut config = project(&root);
        config.service[1].env = Some(HashMap::from([(
            "APP_ENV".to_owned(),
            "staging".to_owned(),
        )]));
        // A scheduler injects `APP_ENV=local`; the app's own env comes later
        // in its container and wins.
        std::fs::write(root.join(".env.staging"), "ASSET_URL=https://cdn.test\n").expect("env");
        let error = check(&config, Some("local")).expect_err("the loaded dotenv");
        assert!(error.contains(".env.staging"), "{error}");

        std::fs::write(root.join(".env.staging"), "APP_DEBUG=true\n").expect("env");
        std::fs::write(root.join(".env"), "ASSET_URL=https://cdn.test\n").expect("env");
        assert_eq!(check(&config, Some("local")), Ok(()), ".env is not loaded");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn apps_without_routes_or_gateway_routes_and_non_laravel_apps_are_left_alone() {
        let root = root("exempt");
        std::fs::write(root.join(".env"), "ASSET_URL=https://cdn.test\n").expect("env");

        let mut unrouted = project(&root);
        unrouted.service[1].routes = None;
        assert_eq!(check(&unrouted, None), Ok(()));

        // `--env` namespaces and test leases have no gateway routes.
        let mut namespaced = project(&root);
        namespaced.service[1].runtime.gateway_https_port = None;
        namespaced.service[1].resolved_domain = None;
        assert_eq!(check(&namespaced, None), Ok(()));
        let mut lease = project(&root);
        lease.service[1].localhost_tls = true;
        assert_eq!(check(&lease, None), Ok(()));

        std::fs::remove_file(root.join("artisan")).expect("rm");
        assert_eq!(check(&project(&root), None), Ok(()), "not a Laravel app");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
