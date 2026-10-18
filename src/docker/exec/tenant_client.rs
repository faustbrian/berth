//! The arguments that run a shared server tenant's client in a throwaway
//! container on the instance network.

use anyhow::Result;

use crate::config::{Driver, ServiceConfig};

/// `docker run` arguments for the tenant's own client (`psql`, `mysql` or
/// `mariadb`, `redis-cli`) in a throwaway container of the server's image.
///
/// The client reaches the server as an app container does: by the service
/// name on the instance network, at its default port, with the tenant's
/// credentials. Its password is set in its environment, never as an
/// argument. A shell escape from the client (`psql`'s `\!`, MySQL's
/// `system`) therefore reaches only this container, not the server every
/// instance shares. It mounts nothing and runs without capabilities.
///
/// # Errors
///
/// Returns an error when the service has no instance network, credentials
/// or tenant client, or the ownership labels cannot be built.
pub(super) fn tenant_client_run_args(service: &ServiceConfig, tty: bool) -> Result<Vec<String>> {
    let network = service
        .runtime
        .network
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' has no instance network", service.name))?;
    let username = service
        .username
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' has no tenant username", service.name))?;
    let password = service
        .password
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' has no tenant password", service.name))?;
    let host = service.name.as_str();
    let port = service.default_port().to_string();
    let (program, password_env, client_args): (&str, &str, Vec<String>) = match service.driver {
        Driver::Postgres => (
            "psql",
            "PGPASSWORD",
            vec![
                "-h".to_owned(),
                host.to_owned(),
                "-p".to_owned(),
                port,
                "-U".to_owned(),
                username.to_owned(),
                database(service)?.to_owned(),
            ],
        ),
        Driver::Mysql => (
            crate::docker::mysql_program(&service.image, crate::docker::MysqlProgram::Client),
            "MYSQL_PWD",
            vec![
                "-h".to_owned(),
                host.to_owned(),
                // `--port=`: berth's redaction treats `-P`'s value as a
                // secret, as it is for some clients.
                format!("--port={port}"),
                "-u".to_owned(),
                username.to_owned(),
                database(service)?.to_owned(),
            ],
        ),
        Driver::Redis | Driver::Valkey => (
            "redis-cli",
            "REDISCLI_AUTH",
            vec![
                "-h".to_owned(),
                host.to_owned(),
                "-p".to_owned(),
                port,
                "--user".to_owned(),
                username.to_owned(),
            ],
        ),
        other => anyhow::bail!(
            "service '{}' ({other:?}) has no tenant client on a shared server",
            service.name
        ),
    };
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    args.push(if tty { "-it" } else { "-i" }.to_owned());
    args.extend(crate::docker::ownership_label_args(
        service,
        &format!("{network}_{}-client", service.name),
        crate::docker::ContainerRole::OneOff,
    )?);
    args.extend([
        "--network".to_owned(),
        network.to_owned(),
        "--cap-drop".to_owned(),
        "ALL".to_owned(),
        "--security-opt".to_owned(),
        "no-new-privileges".to_owned(),
        "-e".to_owned(),
        format!("{password_env}={password}"),
        "--entrypoint".to_owned(),
        program.to_owned(),
        service.image.clone(),
    ]);
    args.extend(client_args);
    Ok(args)
}

fn database(service: &ServiceConfig) -> Result<&str> {
    service
        .database
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("service '{}' has no tenant database", service.name))
}
