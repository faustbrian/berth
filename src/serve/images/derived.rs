//! Derived runtime image planning/build pipeline.
//!
//! Selects when to use base image, cached derived image, or a newly built image
//! based on extension/tooling requirements and rebuild policy.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::javascript::{ResolveJavaScriptRuntimeOptions, resolve_javascript_runtime};
use crate::output::{self, LogLevel, Persistence};
use crate::serve::sql_client_flavor::sql_client_flavor_from_injected_env;

use super::super::image_build::{
    build_derived_image, filter_installable_extensions, render_derived_dockerfile,
    should_include_js_tooling,
};
use super::browser::{browser_test_runtime_enabled, resolve_playwright_package_spec};
use super::lock::{docker_image_exists, read_derived_image_lock, write_derived_image_lock};

/// How long a process waits for another one building the same derived
/// image; a cold extension build takes minutes on a small engine VM.
const DERIVED_BUILD_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30 * 60);
use super::runtime::normalize_php_extensions;
use signature::derive_image_signature;

mod signature;

/// Resolves the runtime image tag for this serve target.
///
/// Prefers cached derived images by signature when available; falls back to base
/// image when no derived requirements exist or rebuild is disallowed.
pub(super) fn resolve_runtime_image(
    target: &ServiceConfig,
    allow_rebuild: bool,
    injected_env: &HashMap<String, String>,
    workspace_root: &Path,
) -> Result<String> {
    let include_js_tooling = should_include_js_tooling(target);
    let playwright_package_spec = browser_test_runtime_enabled(injected_env, &target.name)
        .then(|| resolve_playwright_package_spec(workspace_root))
        .flatten();
    let sql_client_flavor = sql_client_flavor_from_injected_env(injected_env);
    let node_runtime = resolve_javascript_runtime(ResolveJavaScriptRuntimeOptions {
        configured: target.javascript.as_ref(),
        workspace_root,
        runtime: None,
        package_manager: None,
        version_manager: None,
        node_version: None,
        require_package_manager: false,
    })?;
    let normalized_extensions = target
        .php_extensions
        .as_ref()
        .filter(|exts| !exts.is_empty())
        .map(|exts| normalize_php_extensions(exts))
        .unwrap_or_default();

    if normalized_extensions.is_empty() && !include_js_tooling && playwright_package_spec.is_none()
    {
        return Ok(target.image.clone());
    }

    let installable_extensions = if normalized_extensions.is_empty() {
        Vec::new()
    } else {
        filter_installable_extensions(&target.image, &normalized_extensions)?
    };
    if installable_extensions.is_empty() && !include_js_tooling && playwright_package_spec.is_none()
    {
        return Ok(target.image.clone());
    }

    let dockerfile = render_derived_dockerfile(
        &target.image,
        &installable_extensions,
        include_js_tooling,
        node_runtime.runtime,
        node_runtime.version_manager,
        node_runtime.node_version.as_deref(),
        sql_client_flavor,
        playwright_package_spec.as_deref(),
    );
    let signature = derive_image_signature(&dockerfile);
    if let Some(tag) = read_derived_image_lock()?.entries.get(&signature).cloned()
        && docker_image_exists(&tag)?
    {
        emit_derived_event(
            target,
            LogLevel::Info,
            Persistence::Persistent,
            &format!("Using cached derived image {tag}"),
        );
        return Ok(tag);
    }
    if !allow_rebuild {
        emit_derived_event(
            target,
            LogLevel::Warn,
            Persistence::Persistent,
            &format!(
                "Skipped rebuilding derived image because base image is used: {}",
                target.image
            ),
        );
        return Ok(target.image.clone());
    }

    let derived_tag = derived_image_tag(&signature);
    if crate::docker::is_dry_run() {
        emit_derived_event(
            target,
            LogLevel::Info,
            Persistence::Transient,
            &format!(
                "[dry-run] Build derived image {derived_tag} with extensions: {}",
                installable_extensions.join(", ")
            ),
        );
        return Ok(derived_tag);
    }

    // One build per image across processes: parallel `up`s on a cold cache
    // would otherwise compile the same extensions side by side (a 2 GiB
    // Podman VM OOM-killed the second one). A waiter reuses the result.
    let _building = crate::home::HostLock::acquire(
        &format!("derived-image-{}", derived_tag.replace('/', "-")),
        DERIVED_BUILD_LOCK_TIMEOUT,
    )?;
    if docker_image_exists(&derived_tag)? {
        emit_derived_event(
            target,
            LogLevel::Info,
            Persistence::Persistent,
            &format!("Using derived image {derived_tag} another berth process built"),
        );
    } else {
        build_derived_image(&derived_tag, &dockerfile)?;
    }
    let mut lock = read_derived_image_lock()?;
    lock.entries.insert(signature, derived_tag.clone());
    write_derived_image_lock(&lock)?;
    emit_derived_event(
        target,
        LogLevel::Success,
        Persistence::Persistent,
        &format!(
            "Prepared derived image {derived_tag} with extensions: {}",
            installable_extensions.join(", ")
        ),
    );
    Ok(derived_tag)
}

/// Builds a stable derived image tag from the content signature.
pub(super) fn derived_image_tag(signature: &str) -> String {
    signature::derived_image_tag(signature)
}

fn emit_derived_event(
    target: &ServiceConfig,
    level: LogLevel,
    persistence: Persistence,
    message: &str,
) {
    output::event(&target.name, level, message, persistence);
}

#[cfg(test)]
mod tests {
    use super::resolve_runtime_image;
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-derived-browser-audit-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn frankenphp_app() -> ServiceConfig {
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
            domain: Some("app.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: Some(80),
            smtp_port: None,
            volumes: Some(vec![".:/app".to_owned()]),
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
            container_name: Some("audit-app".to_owned()),
            resolved_container_name: Some("audit-app".to_owned()),
            runtime: Default::default(),
        }
    }

    fn mailhog_app() -> ServiceConfig {
        ServiceConfig {
            name: "mailhog".to_owned(),
            kind: Kind::App,
            driver: Driver::Mailhog,
            image: "mailhog/mailhog:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 8025,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("mailhog.berth".to_owned()),
            domains: None,
            resolved_domain: None,
            container_port: Some(8025),
            smtp_port: Some(1025),
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
            container_name: Some("audit-mailhog".to_owned()),
            resolved_container_name: Some("audit-mailhog".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn browser_targeted_frankenphp_runtime_skips_php_module_inspection_when_extensions_are_empty() {
        let root = temp_root("frankenphp");
        fs::write(
            root.join("package.json"),
            r#"{
                "devDependencies": {
                    "playwright": "^1.60.0"
                }
            }"#,
        )
        .expect("write package.json");

        let env = HashMap::from([
            ("BERTH_BROWSER_TEST_RUNTIME".to_owned(), "1".to_owned()),
            (
                "BERTH_BROWSER_TEST_RUNTIME_TARGETS".to_owned(),
                "app".to_owned(),
            ),
        ]);

        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command("/tmp/berth-unexpected-docker", || {
                resolve_runtime_image(&frankenphp_app(), false, &env, &root)
            })
        })
        .expect("resolve runtime image without php -m inspection");

        assert_eq!(result, "dunglas/frankenphp:php8.5");
    }

    #[test]
    fn browser_runtime_marker_is_ignored_for_non_target_app_services() {
        let root = temp_root("mailhog");
        fs::write(root.join("package.json"), "{}").expect("write package.json");

        let env = HashMap::from([
            ("BERTH_BROWSER_TEST_RUNTIME".to_owned(), "1".to_owned()),
            (
                "BERTH_BROWSER_TEST_RUNTIME_TARGETS".to_owned(),
                "app".to_owned(),
            ),
        ]);

        let result = docker::with_dry_run_state(false, || {
            docker::with_docker_command("/tmp/berth-unexpected-docker", || {
                resolve_runtime_image(&mailhog_app(), false, &env, &root)
            })
        })
        .expect("resolve mailhog runtime image without browser targeting");

        assert_eq!(result, "mailhog/mailhog:latest");
    }
}
