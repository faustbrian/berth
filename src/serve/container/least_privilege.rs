//! Least-privilege `docker run` options for PHP app containers (ledger
//! L022–L024, L037; 8.x `application_container_request`).

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

/// Size of Caddy's `/data` and `/config` tmpfs, as in 8.x.
const RUNTIME_TMPFS_BYTES: u64 = 16 * 1024 * 1024;
/// Cap for `HOME`, which holds Composer, npm and Bun caches: a frontend
/// `npm install` writes well over 16 MiB there. tmpfs uses only what is
/// written.
const HOME_TMPFS_BYTES: u64 = 1024 * 1024 * 1024;

/// Whether the target is a PHP runtime that mounts the checkout: the
/// FrankenPHP app and its Horizon, scheduler and Reverb processes. Other
/// app-kind images (Selenium, Gotenberg, Mailpit, Soketi) bring their own
/// users and keep their image defaults.
///
/// An app that terminates TLS itself (container port 443, the only case where
/// `trust_container_ca` acts) keeps 7.0.0's container: its Caddy CA must
/// persist in the image's root-owned `/data` across restarts so host trust
/// stays valid, which neither a tmpfs nor a non-root user allows. Behind the
/// gateway (port 80, the presets' default) it never does, and neither do test
/// leases, which set `localhost_tls` on port 80.
#[must_use]
pub(crate) fn runs_least_privileged(target: &ServiceConfig) -> bool {
    matches!(
        target.driver,
        Driver::Frankenphp | Driver::Horizon | Driver::Scheduler | Driver::Reverb
    ) && target.resolved_container_port() != 443
}

/// The options that run a PHP app container as the invoking user with no
/// capabilities beyond binding ports below 1024:
/// - the invoking user (`container_user`), so files written into the
///   bind-mounted checkout belong to the agent's user instead of root
///   (FI-13); checkout paths that user cannot write fail first, named;
/// - `--cap-drop ALL --cap-add NET_BIND_SERVICE` and `no-new-privileges`:
///   FrankenPHP's binary is capability-marked and fails to start without
///   `NET_BIND_SERVICE` in its bounding set;
/// - writable `noexec,nosuid` tmpfs at `/data` and `/config`, Caddy's state
///   directories, which the image owns as root (FI-44), and a writable,
///   executable `HOME` for Composer, npm and Bun caches and `npx` binaries
///   (`HOME=/` for a user without a passwd entry is not writable).
///
/// # Errors
///
/// Returns an error when the invoking user or the engine's user mode cannot
/// be determined, or when the checkout holds paths that user cannot write.
pub(crate) fn least_privilege_run_args(target: &ServiceConfig) -> Result<Vec<String>> {
    if !runs_least_privileged(target) {
        return Ok(Vec::new());
    }
    let user = super::container_user::invoking_user()?;
    if let Some(root) = target.runtime.root.as_deref() {
        super::checkout_ownership::ensure_checkout_writable_by(root, user.uid, user.gid)?;
    }
    let mut args = super::container_user::container_user_args()?;
    args.extend([
        "--cap-drop".to_owned(),
        "ALL".to_owned(),
        "--cap-add".to_owned(),
        "NET_BIND_SERVICE".to_owned(),
        "--security-opt".to_owned(),
        "no-new-privileges".to_owned(),
    ]);
    for (target, options, size) in [
        ("/data", "rw,noexec,nosuid", RUNTIME_TMPFS_BYTES),
        ("/config", "rw,noexec,nosuid", RUNTIME_TMPFS_BYTES),
        ("/home/berth", "rw,nosuid", HOME_TMPFS_BYTES),
    ] {
        args.push("--tmpfs".to_owned());
        args.push(format!("{target}:{options},mode=1777,size={size}"));
    }
    args.push("--env".to_owned());
    args.push("HOME=/home/berth".to_owned());
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::{least_privilege_run_args, runs_least_privileged};
    use crate::config::{Driver, Kind, ServiceConfig};

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
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
            container_port: Some(80),
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
            container_name: Some("acme-app".to_owned()),
            resolved_container_name: Some("acme-app".to_owned()),
            runtime: Default::default(),
        }
    }

    #[test]
    fn php_runtimes_drop_every_capability_but_port_binding_and_run_as_the_invoker() {
        let args = least_privilege_run_args(&service(Driver::Frankenphp)).expect("args");
        let rendered = args.join(" ");
        let uid = rustix::process::geteuid().as_raw();
        let gid = rustix::process::getegid().as_raw();
        // Unit tests see rootful Docker.
        assert!(
            rendered.starts_with(&format!("--user {uid}:{gid} ")),
            "{rendered}"
        );
        assert!(
            rendered.contains("--cap-drop ALL --cap-add NET_BIND_SERVICE"),
            "{rendered}"
        );
        assert!(
            rendered.contains("--security-opt no-new-privileges"),
            "{rendered}"
        );
        for (mount, options, size) in [
            ("/data", "rw,noexec,nosuid", 16_777_216),
            ("/config", "rw,noexec,nosuid", 16_777_216),
            ("/home/berth", "rw,nosuid", 1_073_741_824),
        ] {
            assert!(
                rendered.contains(&format!("--tmpfs {mount}:{options},mode=1777,size={size}")),
                "{rendered}"
            );
        }
        assert!(rendered.ends_with("--env HOME=/home/berth"), "{rendered}");
        for driver in [Driver::Horizon, Driver::Scheduler, Driver::Reverb] {
            assert!(runs_least_privileged(&service(driver)), "{driver:?}");
        }
    }

    #[test]
    fn in_container_tls_keeps_the_7_0_0_container() {
        // Test leases set `localhost_tls` on port 80: still least-privileged.
        let mut lease = service(Driver::Frankenphp);
        lease.localhost_tls = true;
        assert!(runs_least_privileged(&lease));
        let mut inner_tls = service(Driver::Frankenphp);
        inner_tls.container_port = Some(443);
        assert!(!runs_least_privileged(&inner_tls));
        // The presets ask for CA trust, which acts only on port 443.
        let mut preset = service(Driver::Frankenphp);
        preset.trust_container_ca = true;
        assert!(runs_least_privileged(&preset));
    }

    #[test]
    fn images_with_their_own_users_keep_their_defaults() {
        for driver in [
            Driver::Dusk,
            Driver::Gotenberg,
            Driver::Mailhog,
            Driver::Soketi,
        ] {
            assert!(
                least_privilege_run_args(&service(driver))
                    .expect("args")
                    .is_empty(),
                "{driver:?}"
            );
        }
    }
}
