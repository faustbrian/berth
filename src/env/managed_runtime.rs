//! Runtime values berth injects into Laravel app containers beyond the
//! service endpoints: the config-cache bypass and a managed `APP_KEY`.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::{Config, Driver, Kind, ServiceConfig};

/// Where Laravel caches its configuration inside berth's app containers.
///
/// The workspace is bind-mounted, so a host `bootstrap/cache/config.php`
/// would otherwise freeze the host's values and silently override every
/// injected variable (8.x FI-09). A path outside the mount makes each
/// container read the injected environment.
pub(crate) const LARAVEL_CONFIG_CACHE: &str = "/tmp/berth-laravel-config.php";

/// Placeholder shown in dry runs instead of creating a real key.
const DRY_RUN_APP_KEY: &str = "<berth-managed-app-key>";

/// Adds `APP_CONFIG_CACHE` for Laravel apps, and a managed per-instance
/// `APP_KEY` when neither the app's config nor the project's `.env` sets one.
/// Values already present (explicit app env) are kept.
///
/// # Errors
///
/// Returns an error when the managed key cannot be read or created.
pub(crate) fn apply_managed_runtime_env(
    config: &Config,
    vars: &mut HashMap<String, String>,
) -> Result<()> {
    let Some(app) = config
        .service
        .iter()
        .find(|service| is_laravel_app(service))
    else {
        return Ok(());
    };
    vars.entry("APP_CONFIG_CACHE".to_owned())
        .or_insert_with(|| LARAVEL_CONFIG_CACHE.to_owned());

    // An explicit app key is shared with every container that receives this
    // map (workers, schedulers, one-offs), as 7.0.0's shared `.env` did.
    if let Some(explicit) = app
        .env
        .as_ref()
        .and_then(|env| env.get("APP_KEY"))
        .filter(|value| !value.is_empty())
    {
        vars.entry("APP_KEY".to_owned())
            .or_insert_with(|| explicit.clone());
        return Ok(());
    }
    let dotenv_key = app
        .runtime
        .root
        .as_deref()
        .is_some_and(|root| dotenv_has_app_key(&loaded_dotenv(root, vars.get("APP_ENV"))));
    if vars.contains_key("APP_KEY") || dotenv_key {
        return Ok(());
    }
    let (Some(project), Some(instance)) = (&app.runtime.project, &app.runtime.instance) else {
        return Ok(());
    };
    let key = if crate::docker::is_dry_run() {
        DRY_RUN_APP_KEY.to_owned()
    } else {
        crate::home::app_key(project, instance)?
    };
    crate::docker::register_secret(&key);
    vars.insert("APP_KEY".to_owned(), key);
    Ok(())
}

/// A Laravel application: a FrankenPHP app whose project has an `artisan`
/// file.
pub(crate) fn is_laravel_app(service: &ServiceConfig) -> bool {
    service.kind == Kind::App
        && service.driver == Driver::Frankenphp
        && service
            .runtime
            .root
            .as_deref()
            .is_some_and(|root| root.join("artisan").is_file())
}

/// The dotenv file Laravel loads: `.env.{APP_ENV}` when `APP_ENV` is set
/// and that file exists (test runs use `.env.testing`), else `.env`.
fn loaded_dotenv(root: &Path, app_env: Option<&String>) -> std::path::PathBuf {
    if let Some(name) = app_env.filter(|name| !name.is_empty()) {
        let specific = root.join(format!(".env.{name}"));
        if specific.is_file() {
            return specific;
        }
    }
    root.join(".env")
}

/// Whether a dotenv file sets a non-empty `APP_KEY`, in any form phpdotenv
/// accepts (`export APP_KEY=…`, spaces around `=`, quoted values). A missing
/// or unreadable file sets none.
fn dotenv_has_app_key(path: &Path) -> bool {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return false;
    };
    contents.lines().any(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        line.split_once('=').is_some_and(|(key, value)| {
            key.trim() == "APP_KEY"
                && !value
                    .split(" #")
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .trim_matches(|quote| quote == '"' || quote == '\'')
                    .is_empty()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::{LARAVEL_CONFIG_CACHE, apply_managed_runtime_env};
    use crate::config::{Config, ServiceConfig};
    use std::collections::HashMap;

    fn laravel(root: &std::path::Path) -> Config {
        std::fs::write(root.join("artisan"), "#!/usr/bin/env php\n").expect("artisan");
        let mut app: ServiceConfig = serde_json::from_value(serde_json::json!({
            "name": "app", "kind": "app", "driver": "frankenphp",
            "image": "dunglas/frankenphp", "host": "127.0.0.1", "port": 8000
        }))
        .expect("app");
        app.runtime.root = Some(root.to_path_buf());
        app.runtime.project = Some("api".to_owned());
        app.runtime.instance = Some("a-1".to_owned());
        Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: Some("api".to_owned()),
            domain_strategy: None,
            service: vec![app],
            swarm: vec![],
        }
    }

    fn root(label: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("berth-managed-env-{label}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(&root).expect("root");
        root
    }

    #[test]
    fn laravel_apps_bypass_the_config_cache_and_get_a_managed_key_without_a_dotenv() {
        let root = root("fresh");
        let mut vars = HashMap::new();
        apply_managed_runtime_env(&laravel(&root), &mut vars).expect("env");
        assert_eq!(vars["APP_CONFIG_CACHE"], LARAVEL_CONFIG_CACHE);
        assert!(vars["APP_KEY"].starts_with("base64:"), "{vars:?}");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn an_app_key_in_dotenv_or_app_env_is_never_replaced() {
        let root = root("dotenv");
        std::fs::write(root.join(".env"), "APP_NAME=x\nAPP_KEY=base64:own\n").expect("env");
        let mut vars = HashMap::new();
        apply_managed_runtime_env(&laravel(&root), &mut vars).expect("env");
        assert!(!vars.contains_key("APP_KEY"), "{vars:?}");

        std::fs::write(root.join(".env"), "APP_KEY=\n").expect("env");
        let mut config = laravel(&root);
        config.service[0].env = Some(HashMap::from([(
            "APP_KEY".to_owned(),
            "base64:explicit".to_owned(),
        )]));
        let mut vars = HashMap::new();
        apply_managed_runtime_env(&config, &mut vars).expect("env");
        assert_eq!(
            vars["APP_KEY"], "base64:explicit",
            "the explicit key is shared"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn every_dotenv_spelling_of_an_app_key_counts() {
        let root = root("forms");
        for contents in [
            "export APP_KEY=base64:own\n",
            "APP_KEY = base64:own\n",
            "APP_KEY=\"base64:own\"\n",
            "  export   APP_KEY='base64:own' # mine\n",
        ] {
            std::fs::write(root.join(".env"), contents).expect("env");
            let mut vars = HashMap::new();
            apply_managed_runtime_env(&laravel(&root), &mut vars).expect("env");
            assert!(!vars.contains_key("APP_KEY"), "{contents:?}: {vars:?}");
        }
        for empty in [
            "APP_KEY=\n",
            "APP_KEY=\"\"\n",
            "# APP_KEY=base64:x\n",
            "APP_KEYS=x\n",
        ] {
            std::fs::write(root.join(".env"), empty).expect("env");
            let mut vars = HashMap::new();
            apply_managed_runtime_env(&laravel(&root), &mut vars).expect("env");
            assert!(vars.contains_key("APP_KEY"), "{empty:?}");
        }
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn test_runs_read_the_dotenv_laravel_loads_for_their_app_env() {
        let root = root("testing");
        std::fs::write(root.join(".env"), "APP_KEY=base64:own\n").expect("env");
        std::fs::write(root.join(".env.testing"), "APP_ENV=testing\n").expect("env");
        let mut vars = HashMap::from([("APP_ENV".to_owned(), "testing".to_owned())]);
        apply_managed_runtime_env(&laravel(&root), &mut vars).expect("env");
        assert!(
            vars.contains_key("APP_KEY"),
            ".env.testing has no key: {vars:?}"
        );
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn an_explicit_app_key_is_shared_with_workers() {
        let root = root("explicit");
        let mut config = laravel(&root);
        config.service[0].env = Some(HashMap::from([(
            "APP_KEY".to_owned(),
            "base64:explicit".to_owned(),
        )]));
        let mut vars = HashMap::new();
        apply_managed_runtime_env(&config, &mut vars).expect("env");
        assert_eq!(vars["APP_KEY"], "base64:explicit");
        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn projects_without_artisan_get_nothing() {
        let root = root("plain");
        let mut config = laravel(&root);
        std::fs::remove_file(root.join("artisan")).expect("rm");
        config.service[0].runtime.root = Some(root.clone());
        let mut vars = HashMap::new();
        apply_managed_runtime_env(&config, &mut vars).expect("env");
        assert!(vars.is_empty());
        std::fs::remove_dir_all(&root).expect("cleanup");
    }
}
