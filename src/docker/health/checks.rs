//! docker health checks module.
//!
//! Contains docker health checks logic used by Berth command workflows.

use anyhow::Result;
use std::net::TcpStream;
use std::time::Duration;

use crate::config::{Driver, ServiceConfig};

use super::http::http_status_code;

/// Checks service health and reports actionable failures.
pub(super) fn check_service_health(service: &ServiceConfig, container_name: &str) -> Result<bool> {
    match service.driver {
        Driver::Mongodb => docker_exec_succeeds(
            &[
                "exec",
                container_name,
                "mongosh",
                "--eval",
                "db.adminCommand({ ping: 1 })",
            ],
            "mongodb health check command failed",
        ),
        Driver::Postgres => docker_exec_succeeds(
            &[
                "exec",
                container_name,
                "pg_isready",
                "-U",
                service.username.as_deref().unwrap_or("postgres"),
            ],
            "postgres health check command failed",
        ),
        Driver::Mysql => health_check_mysql(service, container_name),
        Driver::Sqlserver => health_check_tcp(service),
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => docker_exec_succeeds(
            &["exec", container_name, "redis-cli", "PING"],
            "redis health check command failed",
        ),
        Driver::Memcached => health_check_tcp(service),
        Driver::Minio => health_check_http(service, "/minio/health/ready"),
        Driver::Garage => health_check_tcp(service),
        Driver::Rustfs | Driver::Localstack => health_check_http(service, "/"),
        Driver::Opensearch => health_check_http(service, "/"),
        Driver::Elasticsearch => health_check_http(service, "/"),
        Driver::Meilisearch => health_check_http(service, "/health"),
        Driver::Typesense => health_check_http(service, "/health"),
        Driver::Frankenphp => health_check_http(service, "/"),
        Driver::Reverb => health_check_http(service, "/"),
        Driver::Horizon => health_check_horizon(container_name),
        Driver::Scheduler => health_check_scheduler(container_name),
        Driver::Dusk => health_check_http(service, "/wd/hub/status"),
        Driver::Gotenberg => health_check_http(service, "/health"),
        Driver::Mailhog => health_check_http(service, "/"),
        Driver::Rabbitmq => health_check_tcp(service),
        Driver::Soketi => health_check_http(service, "/"),
    }
}

fn health_check_tcp(service: &ServiceConfig) -> Result<bool> {
    if !crate::network::is_published(service) {
        return crate::gateway::tcp_open(service);
    }
    let address = format!("{}:{}", service.host, service.port);
    Ok(TcpStream::connect(address).is_ok())
}

fn health_check_http(service: &ServiceConfig, path: &str) -> Result<bool> {
    if !crate::network::is_published(service) {
        return match crate::gateway::http_status(service, path)? {
            Some(status_code) => Ok(status_code < 500),
            None => anyhow::bail!("{} is not reachable yet", service.name),
        };
    }
    let status_code = http_status_code(&service.host, service.port, path)?;
    Ok(status_code < 500)
}

fn health_check_mysql(service: &ServiceConfig, container_name: &str) -> Result<bool> {
    if !mysql_ping(service, container_name)? {
        return Ok(false);
    }

    // MySQL 8 may report a temporary startup server as ready and then restart.
    // Requiring a second successful ping after a short delay avoids false-ready.
    std::thread::sleep(Duration::from_millis(750));

    mysql_ping(service, container_name)
}

fn mysql_ping(service: &ServiceConfig, container_name: &str) -> Result<bool> {
    let args = vec![
        "exec".to_owned(),
        "-e".to_owned(),
        format!(
            "MYSQL_PWD={}",
            service.password.as_deref().unwrap_or("secret")
        ),
        container_name.to_owned(),
        crate::docker::mysql_program(&service.image, crate::docker::MysqlProgram::Admin).to_owned(),
        "ping".to_owned(),
        "-u".to_owned(),
        service.username.as_deref().unwrap_or("root").to_owned(),
        "--silent".to_owned(),
    ];
    docker_exec_succeeds_owned(&args, "mysql health check command failed")
}

fn health_check_horizon(container_name: &str) -> Result<bool> {
    docker_exec_succeeds(
        &["exec", container_name, "php", "artisan", "horizon:status"],
        "horizon health check command failed",
    )
}

fn health_check_scheduler(container_name: &str) -> Result<bool> {
    docker_exec_succeeds(
        &["exec", container_name, "php", "artisan", "schedule:list"],
        "scheduler health check command failed",
    )
}

fn docker_exec_succeeds(args: &[&str], context: &str) -> Result<bool> {
    crate::docker::run_docker_output(args, context).map(|output| output.status.success())
}

fn docker_exec_succeeds_owned(args: &[String], context: &str) -> Result<bool> {
    let arg_refs = crate::docker::docker_arg_refs(args);
    docker_exec_succeeds(&arg_refs, context)
}

#[cfg(test)]
mod tests {
    use super::check_service_health;
    use crate::config::{Driver, Kind, ServiceConfig};
    use crate::docker;
    use std::env;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::{Duration, Instant};

    fn service(driver: Driver, port: u16) -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver,
            image: "php".to_owned(),
            host: "127.0.0.1".to_owned(),
            port,
            database: None,
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
            container_name: Some("app".to_owned()),
            resolved_container_name: None,
            runtime: Default::default(),
        }
    }

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = env::temp_dir().join(format!(
            "berth-fake-health-docker-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("create temp dir");

        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        use std::io::Write as _;
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let binary = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&binary, test);
        fs::remove_dir_all(&bin_dir).ok();

        result
    }

    fn with_http_server<F, R>(response: &str, requests: usize, test: F) -> R
    where
        F: FnOnce(u16) -> R,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let port = listener.local_addr().expect("listener port").port();
        let response = response.to_owned();
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");

        let handle = thread::spawn(move || {
            let mut remaining = requests;
            let deadline = Instant::now() + Duration::from_secs(2);
            while remaining > 0 && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0_u8; 512];
                        let _read = stream.read(&mut buffer);
                        let _write = stream.write_all(response.as_bytes());
                        let _flush = stream.flush();
                        remaining -= 1;
                    }
                    Err(err)
                        if err.kind() == std::io::ErrorKind::WouldBlock
                            || err.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        let result = test(port);
        handle.join().expect("server thread");
        result
    }

    #[test]
    fn health_check_prefers_execution_backend_by_driver() {
        let drivers = [
            Driver::Mongodb,
            Driver::Postgres,
            Driver::Mysql,
            Driver::Redis,
            Driver::Valkey,
            Driver::Dragonfly,
            Driver::Horizon,
            Driver::Scheduler,
        ];

        with_fake_docker("exit 0", || {
            for driver in drivers {
                let service = service(driver, 3306);
                assert!(check_service_health(&service, "app-container").expect("health check"));
            }
        });
    }

    #[test]
    fn health_check_uses_http_for_http_style_services() {
        let drivers = [
            Driver::Minio,
            Driver::Rustfs,
            Driver::Localstack,
            Driver::Opensearch,
            Driver::Elasticsearch,
            Driver::Meilisearch,
            Driver::Typesense,
            Driver::Frankenphp,
            Driver::Reverb,
            Driver::Dusk,
            Driver::Gotenberg,
            Driver::Mailhog,
            Driver::Soketi,
        ];

        with_http_server("HTTP/1.1 204 No Content\r\n\r\n", drivers.len(), |port| {
            for driver in drivers {
                let service = service(driver, port);
                assert!(check_service_health(&service, "app-container").expect("health check"));
            }
        });
    }

    #[test]
    fn health_check_prefers_tcp_for_tcp_style_services() {
        // Two TCP probes connect, so the server must accept two connections.
        with_http_server("HTTP/1.1 204 No Content\r\n\r\n", 2, |port| {
            let service_sql = service(Driver::Sqlserver, port);
            assert!(check_service_health(&service_sql, "app-container").expect("sql health"));

            let service_garage = service(Driver::Garage, port);
            assert!(check_service_health(&service_garage, "app-container").expect("garage health"));

            let service_rabbit = service(Driver::Rabbitmq, 0);
            assert!(
                !check_service_health(&service_rabbit, "app-container").expect("rabbit health")
            );
        });
    }

    #[test]
    fn health_check_returns_false_when_command_health_fails() {
        with_fake_docker("exit 1", || {
            let service = service(Driver::Mongodb, 3306);
            assert!(!check_service_health(&service, "app-container").expect("failed mongo check"));
        });
    }

    #[test]
    fn health_check_tcp_health_retries_unavailable_targets_as_not_healthy() {
        let service = service(Driver::Sqlserver, 0);
        assert!(!check_service_health(&service, "app-container").expect("unavailable"));
    }
}
