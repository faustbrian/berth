//! config types enums module.
//!
//! Contains config types enums logic used by Berth command workflows.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Berth project archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ProjectType {
    /// Composer app project (Laravel-focused workflows enabled).
    Project,
    /// Composer package/library repository.
    Library,
}

/// Service category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Kind {
    /// SQL databases.
    Database,
    /// Cache backends.
    Cache,
    /// S3-compatible object storage.
    ObjectStore,
    /// Search engines.
    Search,
    /// Application runtimes (web apps, workers, tooling).
    App,
}

/// Service driver/backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Driver {
    /// `MongoDB`.
    Mongodb,
    /// `Memcached`.
    Memcached,
    /// `PostgreSQL`.
    Postgres,
    /// `MySQL`.
    Mysql,
    /// `SQL Server`.
    Sqlserver,
    /// `Redis`.
    Redis,
    /// `Valkey`.
    Valkey,
    /// `Dragonfly`.
    Dragonfly,
    /// `MinIO`.
    Minio,
    /// `Garage`.
    Garage,
    /// `RustFS`.
    Rustfs,
    /// `LocalStack`.
    Localstack,
    /// `OpenSearch`.
    Opensearch,
    /// `Elasticsearch`.
    Elasticsearch,
    /// `Meilisearch`.
    Meilisearch,
    /// `Typesense`.
    Typesense,
    /// App runtime using `FrankenPHP`.
    Frankenphp,
    /// Laravel Reverb WebSocket server.
    Reverb,
    /// Laravel Horizon queue worker process.
    Horizon,
    /// Laravel scheduler worker process.
    Scheduler,
    /// Selenium standalone Chrome container for Laravel Dusk.
    Dusk,
    /// Gotenberg document conversion API.
    Gotenberg,
    /// `MailHog` local email testing server.
    Mailhog,
    /// `RabbitMQ` broker.
    Rabbitmq,
    /// `Soketi` WebSocket server.
    Soketi,
}

/// Container runtime engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ContainerEngine {
    /// Docker engine.
    Docker,
    /// Podman engine.
    Podman,
}

/// Docker container restart policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum RestartPolicy {
    /// Disable automatic restarts.
    No,
    /// Restart on non-zero exit.
    OnFailure,
    /// Always restart, including after manual stop until removed.
    Always,
    /// Restart unless manually stopped.
    UnlessStopped,
}

impl Default for ContainerEngine {
    fn default() -> Self {
        Self::Docker
    }
}

impl RestartPolicy {
    #[must_use]
    pub const fn as_docker_value(self) -> &'static str {
        match self {
            Self::No => "no",
            Self::OnFailure => "on-failure",
            Self::Always => "always",
            Self::UnlessStopped => "unless-stopped",
        }
    }
}

/// Strategy for generating default app domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum DomainStrategy {
    /// Use the kebab-case project directory name as the domain base label.
    Directory,
    /// Use a stable pseudo-random project-specific label as the domain base.
    Random,
}

impl ContainerEngine {
    #[must_use]
    pub const fn command_binary(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    #[must_use]
    pub const fn host_gateway_alias(self) -> &'static str {
        match self {
            Self::Docker => "host.docker.internal",
            Self::Podman => "host.containers.internal",
        }
    }

    #[must_use]
    pub const fn host_gateway_mapping(self) -> Option<&'static str> {
        match self {
            Self::Docker => Some("host.docker.internal:host-gateway"),
            Self::Podman => None,
        }
    }
}
