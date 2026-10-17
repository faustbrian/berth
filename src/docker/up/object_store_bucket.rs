//! Object-store bucket bootstrap for runtime startup.

use anyhow::{Result, anyhow};

use crate::config::{Driver, ServiceConfig};

const AWS_CLI_IMAGE: &str = "amazon/aws-cli:latest";

pub(super) fn ensure_bucket_exists(service: &ServiceConfig) -> Result<()> {
    if !is_object_store_driver(service.driver) {
        return Ok(());
    }

    let bucket = service.bucket.as_deref().unwrap_or("app");
    let container_name = service
        .resolved_container_name
        .as_deref()
        .ok_or_else(|| anyhow!("no resolved container name for service '{}'", service.name))?;
    let access_key = service.access_key.as_deref().unwrap_or("minio");
    let secret_key = service.secret_key.as_deref().unwrap_or("miniosecret");
    let region = service.region.as_deref().unwrap_or("us-east-1");
    let primary = build_container_network_bootstrap(
        service,
        bucket,
        access_key,
        secret_key,
        region,
        container_name,
    );
    let legacy = build_host_gateway_bootstrap(service, bucket, access_key, secret_key, region);

    if crate::docker::is_dry_run() {
        crate::docker::print_docker_command(&primary.head_args);
        crate::docker::print_docker_command(&primary.create_args);
        return Ok(());
    }

    match run_bucket_bootstrap(service, bucket, &primary) {
        Ok(()) => Ok(()),
        Err(primary_error) => {
            let Some(legacy) = legacy else {
                return Err(primary_error);
            };
            run_bucket_bootstrap(service, bucket, &legacy).map_err(|legacy_error| {
                anyhow!(
                    "Failed to ensure object-store bucket '{}' for service '{}'. \
                     Primary bootstrap failed: {}. Fallback bootstrap failed: {}",
                    bucket,
                    service.name,
                    primary_error,
                    legacy_error
                )
            })
        }
    }
}

fn is_object_store_driver(driver: Driver) -> bool {
    matches!(
        driver,
        Driver::Minio | Driver::Garage | Driver::Rustfs | Driver::Localstack
    )
}

struct BucketBootstrapArgs {
    head_args: Vec<String>,
    create_args: Vec<String>,
}

fn build_container_network_bootstrap(
    service: &ServiceConfig,
    bucket: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    container_name: &str,
) -> BucketBootstrapArgs {
    let endpoint = format!("http://localhost:{}", service.default_port());
    BucketBootstrapArgs {
        head_args: build_container_network_aws_cli_args(
            "head-bucket",
            bucket,
            &endpoint,
            access_key,
            secret_key,
            region,
            container_name,
        ),
        create_args: build_container_network_aws_cli_create_args(
            bucket,
            &endpoint,
            access_key,
            secret_key,
            region,
            container_name,
        ),
    }
}

fn build_host_gateway_bootstrap(
    service: &ServiceConfig,
    bucket: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Option<BucketBootstrapArgs> {
    let mapping = crate::docker::host_gateway_mapping()?;
    let endpoint = format!(
        "http://{}:{}",
        crate::docker::host_gateway_alias(),
        service.port
    );
    Some(BucketBootstrapArgs {
        head_args: build_host_gateway_aws_cli_args(
            "head-bucket",
            bucket,
            &endpoint,
            access_key,
            secret_key,
            region,
            mapping,
        ),
        create_args: build_host_gateway_aws_cli_create_args(
            bucket, &endpoint, access_key, secret_key, region, mapping,
        ),
    })
}

fn run_bucket_bootstrap(
    service: &ServiceConfig,
    bucket: &str,
    args: &BucketBootstrapArgs,
) -> Result<()> {
    let head = crate::docker::run_docker_output_owned(
        &args.head_args,
        &crate::docker::runtime_command_error_context("run"),
    )?;
    if head.status.success() {
        return Ok(());
    }

    let create = crate::docker::run_docker_output_owned(
        &args.create_args,
        &crate::docker::runtime_command_error_context("run"),
    )?;
    if create.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&create.stderr).trim().to_owned();
    if stderr.contains("BucketAlreadyOwnedByYou") || stderr.contains("BucketAlreadyExists") {
        return Ok(());
    }

    Err(anyhow!(
        "Failed to ensure object-store bucket '{}' for service '{}': {}",
        bucket,
        service.name,
        stderr
    ))
}

fn build_container_network_aws_cli_args(
    operation: &str,
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    container_name: &str,
) -> Vec<String> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    args.push("--network".to_owned());
    args.push(format!("container:{container_name}"));
    args.extend([
        "-e".to_owned(),
        format!("AWS_ACCESS_KEY_ID={access_key}"),
        "-e".to_owned(),
        format!("AWS_SECRET_ACCESS_KEY={secret_key}"),
        "-e".to_owned(),
        format!("AWS_DEFAULT_REGION={region}"),
        AWS_CLI_IMAGE.to_owned(),
        "s3api".to_owned(),
        operation.to_owned(),
        "--bucket".to_owned(),
        bucket.to_owned(),
        "--endpoint-url".to_owned(),
        endpoint.to_owned(),
    ]);
    args
}

fn build_container_network_aws_cli_create_args(
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    container_name: &str,
) -> Vec<String> {
    let mut args = build_container_network_aws_cli_args(
        "create-bucket",
        bucket,
        endpoint,
        access_key,
        secret_key,
        region,
        container_name,
    );
    if region != "us-east-1" {
        args.push("--create-bucket-configuration".to_owned());
        args.push(format!("LocationConstraint={region}"));
    }
    args
}

fn build_host_gateway_aws_cli_args(
    operation: &str,
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    mapping: &str,
) -> Vec<String> {
    let mut args = vec!["run".to_owned(), "--rm".to_owned()];
    args.push("--add-host".to_owned());
    args.push(mapping.to_owned());
    args.extend([
        "-e".to_owned(),
        format!("AWS_ACCESS_KEY_ID={access_key}"),
        "-e".to_owned(),
        format!("AWS_SECRET_ACCESS_KEY={secret_key}"),
        "-e".to_owned(),
        format!("AWS_DEFAULT_REGION={region}"),
        AWS_CLI_IMAGE.to_owned(),
        "s3api".to_owned(),
        operation.to_owned(),
        "--bucket".to_owned(),
        bucket.to_owned(),
        "--endpoint-url".to_owned(),
        endpoint.to_owned(),
    ]);
    args
}

fn build_host_gateway_aws_cli_create_args(
    bucket: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
    mapping: &str,
) -> Vec<String> {
    let mut args = build_host_gateway_aws_cli_args(
        "create-bucket",
        bucket,
        endpoint,
        access_key,
        secret_key,
        region,
        mapping,
    );
    if region != "us-east-1" {
        args.push("--create-bucket-configuration".to_owned());
        args.push(format!("LocationConstraint={region}"));
    }
    args
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::config::{Driver, Kind, ServiceConfig};

    use super::ensure_bucket_exists;

    static UNIQUE_SUFFIX_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_suffix() -> String {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0_u128, |dur| dur.as_nanos());
        let nonce = UNIQUE_SUFFIX_COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("{stamp}-{nonce}")
    }

    fn service(driver: Driver) -> ServiceConfig {
        ServiceConfig {
            name: "s3".to_owned(),
            kind: Kind::ObjectStore,
            driver,
            image: "rustfs/rustfs:latest".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 9000,
            database: None,
            username: None,
            password: None,
            bucket: Some("media".to_owned()),
            access_key: Some("minio".to_owned()),
            secret_key: Some("miniosecret".to_owned()),
            api_key: None,
            region: Some("us-east-1".to_owned()),
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
            container_name: Some("acme-s3".to_owned()),
            resolved_container_name: Some("acme-s3".to_owned()),
        }
    }

    fn with_fake_runtime_command<T>(script: &str, test: impl FnOnce() -> T) -> T {
        let dir = std::env::temp_dir().join(format!("berth-fake-runtime-{}", unique_suffix()));
        fs::create_dir_all(&dir).expect("create temp runtime dir");
        let binary = dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake runtime binary");
        writeln!(file, "#!/bin/sh\n{script}").expect("write fake runtime script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod fake runtime binary");
        }
        let runtime_bin = binary.to_string_lossy().to_string();

        let result = crate::docker::with_docker_command(&runtime_bin, test);
        fs::remove_dir_all(&dir).ok();
        result
    }

    #[test]
    fn ensure_bucket_exists_skips_non_object_store_drivers() {
        with_fake_runtime_command("exit 99", || {
            let mut svc = service(Driver::Mysql);
            svc.kind = Kind::Database;
            ensure_bucket_exists(&svc).expect("should skip non object-store service");
        });
    }

    #[test]
    fn ensure_bucket_exists_creates_bucket_when_missing() {
        with_fake_runtime_command(
            r#"
if echo "$*" | grep -q "head-bucket"; then
  echo "Not Found" 1>&2
  exit 1
fi
if echo "$*" | grep -q "create-bucket"; then
  exit 0
fi
exit 1
"#,
            || {
                ensure_bucket_exists(&service(Driver::Rustfs))
                    .expect("should create missing object-store bucket");
            },
        );
    }

    #[test]
    fn ensure_bucket_exists_tolerates_bucket_already_owned_on_create() {
        with_fake_runtime_command(
            r#"
if echo "$*" | grep -q "head-bucket"; then
  echo "Not Found" 1>&2
  exit 1
fi
if echo "$*" | grep -q "create-bucket"; then
  echo "BucketAlreadyOwnedByYou" 1>&2
  exit 1
fi
exit 1
"#,
            || {
                ensure_bucket_exists(&service(Driver::Minio))
                    .expect("already-owned bucket should not fail");
            },
        );
    }

    #[test]
    fn ensure_bucket_exists_uses_container_network_localhost_endpoint() {
        let log = std::env::temp_dir().join(format!("berth-bucket-args-{}.log", unique_suffix()));
        let log_path = log.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs)).expect("bucket check should pass");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--endpoint-url http://localhost:9000"));
        fs::remove_file(log).ok();
    }

    #[test]
    fn ensure_bucket_exists_uses_container_network_for_connectivity() {
        let log =
            std::env::temp_dir().join(format!("berth-bucket-host-map-{}.log", unique_suffix()));
        let log_path = log.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs)).expect("bucket check should pass");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--network container:acme-s3"));
        fs::remove_file(log).ok();
    }

    #[test]
    fn ensure_bucket_exists_falls_back_to_host_gateway_when_container_network_fails() {
        let log =
            std::env::temp_dir().join(format!("berth-bucket-fallback-{}.log", unique_suffix()));
        let log_path = log.to_string_lossy().to_string();
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q -- "--network container:acme-s3"; then
  echo "network mode container: unsupported" 1>&2
  exit 125
fi
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&service(Driver::Rustfs))
                    .expect("legacy fallback should pass");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--network container:acme-s3"));
        assert!(content.contains("--add-host host.docker.internal:host-gateway"));
        fs::remove_file(log).ok();
    }

    #[test]
    fn ensure_bucket_exists_fallback_uses_published_host_port() {
        let log = std::env::temp_dir().join(format!(
            "berth-bucket-port-fallback-{}.log",
            unique_suffix()
        ));
        let log_path = log.to_string_lossy().to_string();
        let mut svc = service(Driver::Rustfs);
        svc.port = 49123;
        with_fake_runtime_command(
            &format!(
                r#"
echo "$*" >> "{}"
if echo "$*" | grep -q -- "--network container:acme-s3"; then
  echo "network mode container: unsupported" 1>&2
  exit 125
fi
if echo "$*" | grep -q "head-bucket"; then
  exit 0
fi
exit 1
"#,
                log_path
            ),
            || {
                ensure_bucket_exists(&svc).expect("legacy fallback should use published port");
            },
        );

        let content = fs::read_to_string(Path::new(&log)).expect("read fake runtime log");
        assert!(content.contains("--endpoint-url http://host.docker.internal:49123"));
        fs::remove_file(log).ok();
    }
}
