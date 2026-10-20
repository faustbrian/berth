//! Trust-store integration for inner container TLS CAs.
//!
//! This flow extracts the container-local Caddy root cert and installs it into
//! the host trust store when safe and supported.

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};
use anyhow::Result;

mod cert;
mod command;
mod container;
mod install;
mod keychain;
mod precheck;

use cert::cert_fingerprint_sha256;
use container::{copy_container_ca_cert, find_container_ca_with_warmup};
use install::install_cert_trust;
use keychain::is_cert_trusted_in_system_keychain;
use precheck::evaluate_trust_preconditions;

/// Attempts to trust the inner Caddy CA used by a served app container.
///
/// This is best-effort: it may skip with informative logs when prerequisites
/// are not met.
pub(super) fn trust_inner_container_ca(target: &ServiceConfig, detached: bool) -> Result<()> {
    let Some((container_name, output_path)) = evaluate_trust_preconditions(target)? else {
        return Ok(());
    };

    let cert_path = find_container_ca_with_warmup(target, &container_name)?.ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate container CA certificate. run:\n\
             curl -kI https://{}:{}\n\
             then retry {}",
            target.host,
            target.port,
            crate::followup::for_service(
                target,
                &format!("serve --service {} --trust-container-ca", target.name)
            )
        )
    })?;

    copy_container_ca_cert(&container_name, &cert_path, &output_path)?;

    let fingerprint = cert_fingerprint_sha256(&output_path)?;
    if is_cert_trusted_in_system_keychain(&fingerprint)? {
        output::event(
            &target.name,
            LogLevel::Info,
            "Inner container CA already trusted",
            Persistence::Persistent,
        );
        return Ok(());
    }

    install_cert_trust(&output_path, detached)
}
