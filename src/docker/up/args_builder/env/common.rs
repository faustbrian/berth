//! Common docker run env and volume argument helpers.

use crate::config::{Driver, ServiceConfig};

/// Appends volumes to the caller-provided command or collection.
pub(super) fn append_volumes(
    args: &mut Vec<String>,
    service: &ServiceConfig,
    container_name: &str,
) {
    if let Some(volumes) = &service.volumes {
        for volume in volumes {
            args.push("-v".to_owned());
            args.push(volume.clone());
        }
        return;
    }

    if let Some(data_dir) = default_persistent_data_dir(service) {
        args.push("-v".to_owned());
        args.push(format!("{container_name}-data:{data_dir}"));
    }
}

/// The named Docker volumes `append_volumes` mounts: the default data volume
/// or the authored entries whose source is a name rather than a host path.
pub(crate) fn named_volumes(service: &ServiceConfig, container_name: &str) -> Vec<String> {
    if let Some(volumes) = &service.volumes {
        return volumes
            .iter()
            .filter_map(|volume| volume.split(':').next())
            .filter(|source| {
                !source.is_empty()
                    && !source.contains('/')
                    && !source.starts_with('.')
                    && !source.starts_with('~')
            })
            .map(ToOwned::to_owned)
            .collect();
    }
    default_persistent_data_dir(service)
        .map(|_| vec![format!("{container_name}-data")])
        .unwrap_or_default()
}

/// Appends custom env to the caller-provided command or collection.
pub(super) fn append_custom_env(args: &mut Vec<String>, service: &ServiceConfig) {
    if let Some(custom_env) = &service.env {
        for (key, value) in custom_env {
            args.push("-e".to_owned());
            args.push(format!("{key}={value}"));
        }
    }
}

/// Returns the data mount for a PostgreSQL image.
///
/// Official `postgres` images from major 18 set `PGDATA` below a version
/// directory and declare `VOLUME /var/lib/postgresql`; their entrypoint refuses
/// to start when `/var/lib/postgresql/data` is a mount point
/// (docker-library/postgres `docker-entrypoint.sh`, `OLD_DATABASES` check).
/// Only a tag that names major 18 or later selects the new path: an untagged,
/// `latest` or digest-only reference may be a cached pre-18 image whose data
/// already lives at the classic path, so it keeps the 7.0 mount unchanged.
/// Older majors and other PostgreSQL distributions keep the classic path.
fn postgres_data_dir(image: &str) -> &'static str {
    let (repository, tag) = split_image_reference(image);
    let official = matches!(
        repository,
        "postgres" | "library/postgres" | "docker.io/library/postgres" | "docker.io/postgres"
    );
    if !official {
        return "/var/lib/postgresql/data";
    }
    let major = tag
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse::<u32>()
        .ok();
    match major {
        Some(18..) => "/var/lib/postgresql",
        _ => "/var/lib/postgresql/data",
    }
}

/// Splits `repository[:tag][@digest]` into repository and tag (`latest` when
/// absent). A registry port such as `host:5000/x` is not mistaken for a tag.
fn split_image_reference(image: &str) -> (&str, &str) {
    let without_digest = image.split('@').next().unwrap_or(image);
    match without_digest.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => (repository, tag),
        _ => (without_digest, "latest"),
    }
}

fn default_persistent_data_dir(service: &ServiceConfig) -> Option<&'static str> {
    match service.driver {
        Driver::Mongodb => Some("/data/db"),
        Driver::Postgres => Some(postgres_data_dir(&service.image)),
        Driver::Mysql => Some("/var/lib/mysql"),
        Driver::Sqlserver => Some("/var/opt/mssql"),
        Driver::Redis | Driver::Valkey | Driver::Dragonfly => Some("/data"),
        Driver::Minio | Driver::Rustfs => Some("/data"),
        Driver::Garage => Some("/var/lib/garage"),
        Driver::Localstack => Some("/var/lib/localstack"),
        Driver::Opensearch => Some("/usr/share/opensearch/data"),
        Driver::Elasticsearch => Some("/usr/share/elasticsearch/data"),
        Driver::Meilisearch => Some("/meili_data"),
        Driver::Typesense => Some("/data"),
        Driver::Memcached
        | Driver::Frankenphp
        | Driver::Reverb
        | Driver::Horizon
        | Driver::Scheduler
        | Driver::Dusk
        | Driver::Gotenberg
        | Driver::Mailhog
        | Driver::Rabbitmq
        | Driver::Soketi => None,
    }
}

#[cfg(test)]
mod tests {
    use super::postgres_data_dir;

    #[test]
    fn official_postgres_18_and_later_mount_the_parent_data_directory() {
        assert_eq!(postgres_data_dir("postgres:18"), "/var/lib/postgresql");
        assert_eq!(
            postgres_data_dir("postgres:18.1-alpine"),
            "/var/lib/postgresql"
        );
        assert_eq!(
            postgres_data_dir("docker.io/library/postgres:19"),
            "/var/lib/postgresql"
        );
        assert_eq!(
            postgres_data_dir(
                "postgres:18@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            ),
            "/var/lib/postgresql"
        );
    }

    #[test]
    fn older_majors_and_other_distributions_keep_the_classic_path() {
        assert_eq!(postgres_data_dir("postgres:17"), "/var/lib/postgresql/data");
        // Unknown majors keep the 7.0 mount so existing data is never hidden.
        assert_eq!(postgres_data_dir("postgres"), "/var/lib/postgresql/data");
        assert_eq!(
            postgres_data_dir("postgres:latest"),
            "/var/lib/postgresql/data"
        );
        assert_eq!(
            postgres_data_dir(
                "postgres@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ),
            "/var/lib/postgresql/data"
        );
        assert_eq!(
            postgres_data_dir("postgres:16-alpine"),
            "/var/lib/postgresql/data"
        );
        assert_eq!(
            postgres_data_dir("timescale/timescaledb-ha:pg18"),
            "/var/lib/postgresql/data"
        );
        assert_eq!(
            postgres_data_dir("registry.local:5000/postgres"),
            "/var/lib/postgresql/data"
        );
    }
}
