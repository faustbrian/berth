//! Explicit conversion of helm, stackctl 6/7 and stackctl 8 project files into
//! berth 9.x `.berth.yaml`.
//!
//! Conversion is deliberately conservative: every field is either mapped to its
//! exact 9.x equivalent or rejected with the field named, so a migrated file
//! never silently changes meaning.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::schema::{MAX_PROJECT_NAME_LEN, parse_project_file, validate_project_name};
use super::{LEGACY_CONFIG_FILE_NAMES, PROJECT_FILE_NAME, PROJECT_SCHEMA_VERSION};

/// Result of converting one legacy file.
#[derive(Debug)]
pub(crate) struct ConvertedProjectFile {
    /// Rendered `.berth.yaml` content.
    pub(crate) content: String,
    /// Human-readable notes about values that were derived or dropped.
    pub(crate) notes: Vec<String>,
}

/// One file written by `berth config migrate`.
#[derive(Debug)]
pub(crate) struct MigratedProjectFile {
    pub(crate) source: PathBuf,
    pub(crate) target: PathBuf,
    pub(crate) notes: Vec<String>,
}

#[derive(Serialize)]
struct RenderedProjectFile {
    schema_version: u32,
    project: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_type: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    container_engine: Option<Value>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    services: Map<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    swarm: Vec<Value>,
}

/// Converts legacy project file content into `.berth.yaml` content.
///
/// `legacy_path` selects the input format by extension and supplies the
/// directory used to derive a project name when the file has none.
///
/// # Errors
///
/// Returns an error when the input cannot be parsed or contains a field that
/// has no exact 9.x equivalent.
pub(crate) fn convert_legacy_config(
    content: &str,
    legacy_path: &Path,
) -> Result<ConvertedProjectFile> {
    let document = parse_legacy_document(content, legacy_path)?;
    let Value::Object(mut root) = document else {
        anyhow::bail!(
            "{} is not a mapping at the top level",
            legacy_path.display()
        );
    };
    let mut notes = Vec::new();

    let rendered = match root.remove("schema_version") {
        None => convert_v7(root, legacy_path, &mut notes)?,
        Some(version) => match version.as_u64() {
            Some(1) => convert_v7(root, legacy_path, &mut notes)?,
            Some(8) => convert_v8(root, legacy_path, &mut notes)?,
            _ => anyhow::bail!(
                "{} declares schema_version {version}; only helm/stackctl schema 1 and stackctl 8 files can be migrated",
                legacy_path.display()
            ),
        },
    };

    let content =
        crate::config::yaml::to_string(&rendered).context("failed to render .berth.yaml")?;
    parse_project_file(&content, &legacy_path.with_file_name(PROJECT_FILE_NAME))
        .context("converted project file failed berth 9 validation")?;
    Ok(ConvertedProjectFile { content, notes })
}

/// Converts every legacy project file in `dir` into its berth 9 name.
///
/// The base project file and runtime-environment overrides
/// (`.stackctl.testing.toml` and similar) are converted. Existing berth 9
/// files are never overwritten.
///
/// # Errors
///
/// Returns an error when no legacy file exists, a target already exists, or a
/// conversion fails. No file is written unless every conversion succeeds.
pub(crate) fn migrate_project_file(dir: &Path) -> Result<Vec<MigratedProjectFile>> {
    let sources = legacy_sources(dir)?;
    if sources.is_empty() {
        anyhow::bail!(
            "no legacy project file found in {} (looked for {})",
            dir.display(),
            LEGACY_CONFIG_FILE_NAMES.join(", ")
        );
    }

    let mut planned: Vec<(PathBuf, PathBuf, ConvertedProjectFile)> =
        Vec::with_capacity(sources.len());
    for (source, target_name) in sources {
        let target = dir.join(&target_name);
        if let Some((other, _, _)) = planned
            .iter()
            .find(|(_, planned_target, _)| *planned_target == target)
        {
            anyhow::bail!(
                "both {} and {} would become {}; keep exactly one before migrating",
                other.display(),
                source.display(),
                target.display()
            );
        }
        if target.exists() {
            anyhow::bail!(
                "{} already exists; remove it or migrate by hand",
                target.display()
            );
        }
        let content = crate::config::yaml::read_bounded_file(&source)?;
        let converted = convert_legacy_config(&content, &source)
            .with_context(|| format!("failed to convert {}", source.display()))?;
        planned.push((source, target, converted));
    }

    if let Some(lock) = LEGACY_LOCKFILE_NAMES
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
        && let Some((_, _, base)) = planned.iter_mut().find(|(_, target, _)| {
            target.file_name().and_then(|name| name.to_str()) == Some(PROJECT_FILE_NAME)
        })
    {
        base.notes.push(format!(
            "{} is not converted; run `berth lock images` to create .berth.lock.yaml",
            lock.display()
        ));
    }

    write_all_or_nothing(planned)
}

/// Stages every converted file under a temporary name, then renames them into
/// place. Any failure removes what this call created, so a failed migration
/// never leaves a partial set of berth 9 files behind.
fn write_all_or_nothing(
    planned: Vec<(PathBuf, PathBuf, ConvertedProjectFile)>,
) -> Result<Vec<MigratedProjectFile>> {
    let mut staged: Vec<PathBuf> = Vec::with_capacity(planned.len());
    let mut published: Vec<PathBuf> = Vec::with_capacity(planned.len());
    let result = (|| -> Result<()> {
        for (_, target, converted) in &planned {
            let stage = staging_path(target);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stage)
                .with_context(|| format!("failed to create {}", stage.display()))?;
            staged.push(stage.clone());
            file.write_all(converted.content.as_bytes())
                .and_then(|()| file.sync_all())
                .with_context(|| format!("failed to write {}", stage.display()))?;
        }
        for ((_, target, _), stage) in planned.iter().zip(&staged) {
            if target.exists() {
                anyhow::bail!(
                    "{} appeared during migration; nothing was changed",
                    target.display()
                );
            }
            std::fs::rename(stage, target)
                .with_context(|| format!("failed to publish {}", target.display()))?;
            published.push(target.clone());
        }
        Ok(())
    })();

    if let Err(error) = result {
        for path in staged.iter().chain(&published) {
            drop(std::fs::remove_file(path));
        }
        return Err(error);
    }

    Ok(planned
        .into_iter()
        .map(|(source, target, converted)| MigratedProjectFile {
            source,
            target,
            notes: converted.notes,
        })
        .collect())
}

fn staging_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or(PROJECT_FILE_NAME);
    target.with_file_name(format!("{name}.berth-migrate-{}", std::process::id()))
}

/// Lockfiles from earlier releases; they are regenerated, never converted.
const LEGACY_LOCKFILE_NAMES: [&str; 4] = [
    ".helm.lock.toml",
    ".stackctl.lock.toml",
    ".stackctl.lock.yaml",
    ".berth.lock.toml",
];

/// Returns whether `file_name` is a project file that `config migrate` reads.
pub(crate) fn is_migratable_file_name(file_name: &str) -> bool {
    LEGACY_CONFIG_FILE_NAMES.contains(&file_name) || runtime_env_override_name(file_name).is_some()
}

/// 8.x already shared PostgreSQL and MySQL-family servers and named its logical database
/// from `database`; berth 9 names the tenant per instance, so the name is
/// dropped rather than turning the service into a dedicated container.
fn drop_v8_shared_database_name(service: &mut Map<String, Value>) -> bool {
    let shared = matches!(
        service.get("preset").and_then(Value::as_str),
        Some("postgres" | "mysql" | "mariadb")
    );
    shared && service.remove("database").is_some()
}

/// A PostgreSQL, MySQL or MariaDB service with authored credentials keeps its 7.0.0 dedicated
/// container: a shared server's tenant gets generated credentials, and the
/// project's code or tooling may rely on the authored ones.
fn keep_dedicated_database(service: &mut Map<String, Value>) -> bool {
    let shared_by_default = ["postgres", "mysql", "mariadb"]
        .iter()
        .any(|engine| service.get("preset").and_then(Value::as_str) == Some(*engine))
        || ["postgres", "mysql"]
            .iter()
            .any(|engine| service.get("driver").and_then(Value::as_str) == Some(*engine));
    let authored = [
        "database",
        "username",
        "password",
        "volumes",
        "env",
        "port",
        "container_port",
        "restart",
        "hook",
    ]
    .iter()
    .any(|key| service.contains_key(*key));
    if !shared_by_default || !authored || service.contains_key("isolation") {
        return false;
    }
    service.insert(
        "isolation".to_owned(),
        Value::String("dedicated".to_owned()),
    );
    true
}

/// Removes `domain`/`domains` values berth 9 cannot serve (anything but one
/// label under `berth.localhost`), returning the dropped values. Copying them
/// would produce a file that fails to load.
fn drop_unservable_domains(service_name: &str, service: &mut Map<String, Value>) -> Vec<String> {
    let servable =
        |domain: &str| crate::gateway::validate_authored_domain(service_name, domain).is_ok();
    let mut dropped = Vec::new();
    if let Some(Value::String(domain)) = service.get("domain")
        && !servable(domain)
    {
        dropped.push(domain.clone());
        service.remove("domain");
    }
    if let Some(Value::Array(domains)) = service.get_mut("domains") {
        domains.retain(|value| match value {
            Value::String(domain) if !servable(domain) => {
                dropped.push(domain.clone());
                false
            }
            _ => true,
        });
        if domains.is_empty() {
            service.remove("domains");
        }
    }
    dropped
}

/// Test fixture writer with the signature of `std::fs::write`.
///
/// Converts 7.0-style TOML fixture content into a berth 9 project file at
/// `path`, so existing fixtures exercise the same converter as
/// `berth config migrate`.
#[cfg(test)]
pub(crate) fn write_test_config_file<P, C>(path: P, legacy_toml: C) -> std::io::Result<()>
where
    P: AsRef<Path>,
    C: AsRef<str>,
{
    let path = path.as_ref();
    let source = path.with_file_name(".stackctl.toml");
    let converted = convert_legacy_config(legacy_toml.as_ref(), &source).map_err(|error| {
        std::io::Error::other(format!("test fixture did not convert: {error:#}"))
    })?;
    std::fs::write(path, converted.content)
}

fn legacy_sources(dir: &Path) -> Result<Vec<(PathBuf, String)>> {
    let mut sources = Vec::new();
    let mut base_found = false;
    for name in LEGACY_CONFIG_FILE_NAMES {
        let path = dir.join(name);
        if path.is_file() {
            if base_found {
                anyhow::bail!(
                    "found more than one legacy project file in {}; keep exactly one before migrating",
                    dir.display()
                );
            }
            base_found = true;
            sources.push((path, PROJECT_FILE_NAME.to_owned()));
        }
    }

    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("failed to read directory {}", dir.display()))?;
    for entry in entries {
        let entry = entry.with_context(|| format!("failed to read entry in {}", dir.display()))?;
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        if let Some(env_name) = runtime_env_override_name(&name) {
            sources.push((entry.path(), format!(".berth.{env_name}.yaml")));
        }
    }
    Ok(sources)
}

/// Returns the runtime environment name of a legacy override file such as
/// `.stackctl.testing.toml`, excluding lock files.
fn runtime_env_override_name(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_suffix(".toml")
        .or_else(|| file_name.strip_suffix(".yaml"))?;
    let env_name = stem
        .strip_prefix(".helm.")
        .or_else(|| stem.strip_prefix(".stackctl."))
        .or_else(|| stem.strip_prefix(".berth."))?;
    if env_name.is_empty() || env_name == "lock" || env_name.contains('.') {
        return None;
    }
    if file_name.starts_with(".berth.") && file_name.ends_with(".yaml") {
        return None;
    }
    Some(env_name)
}

fn parse_legacy_document(content: &str, path: &Path) -> Result<Value> {
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
    if name.ends_with(".toml") {
        let value: toml::Value = toml::from_str(content)
            .with_context(|| format!("invalid TOML in {}", path.display()))?;
        return serde_json::to_value(value).context("failed to normalize TOML document");
    }
    if name.ends_with(".yaml") {
        return crate::config::yaml::parse_text_tree(content)
            .with_context(|| format!("invalid YAML in {}", path.display()));
    }
    anyhow::bail!("unsupported legacy project file {}", path.display())
}

fn convert_v7(
    mut root: Map<String, Value>,
    legacy_path: &Path,
    notes: &mut Vec<String>,
) -> Result<RenderedProjectFile> {
    let project = match root.remove("container_prefix") {
        Some(Value::String(prefix)) => prefix,
        Some(other) => anyhow::bail!("`container_prefix` must be a string, found {other}"),
        None => {
            let derived = derive_project_name(legacy_path)?;
            notes.push(format!(
                "no `container_prefix` was set; derived project name '{derived}' from the directory name"
            ));
            derived
        }
    };
    validate_project_name(&project).with_context(|| {
        format!("`container_prefix` '{project}' cannot become the berth 9 project name")
    })?;

    match root.remove("domain_strategy") {
        None => notes.push(
            "berth 9 gives every web app an instance route `{instance}-{project}[-{service}].berth.localhost`"
                .to_owned(),
        ),
        Some(Value::String(strategy)) if strategy == "directory" => {}
        Some(other) => anyhow::bail!(
            "`domain_strategy` {other} has no berth 9 equivalent; berth 9 routes are derived from the instance"
        ),
    }

    let project_type = root.remove("project_type");
    let container_engine = root.remove("container_engine");

    let mut services = Map::new();
    let mut dropped_container_names = false;
    if let Some(list) = root.remove("service") {
        let Value::Array(items) = list else {
            anyhow::bail!("`service` must be a list of service tables");
        };
        for item in items {
            let Value::Object(mut service) = item else {
                anyhow::bail!("every `service` entry must be a table");
            };
            if service.remove("container_name").is_some() {
                dropped_container_names = true;
            }
            let name = match service.remove("name") {
                Some(Value::String(name)) => name,
                Some(other) => anyhow::bail!("service `name` must be a string, found {other}"),
                None => default_service_name(&service)?,
            };
            if services.contains_key(&name) {
                anyhow::bail!("service '{name}' is declared more than once");
            }
            if keep_dedicated_database(&mut service) {
                notes.push(format!(
                    "service '{name}' sets its own database credentials or container settings, \
                     so it keeps a dedicated container (`isolation: dedicated`) instead of a shared server's \
                     generated tenant"
                ));
            }
            let dropped = drop_unservable_domains(&name, &mut service);
            if !dropped.is_empty() {
                notes.push(format!(
                    "service '{name}': dropped domain(s) {} — berth 9 serves only one label \
                     under berth.localhost and otherwise uses the generated instance route",
                    dropped.join(", ")
                ));
            }
            services.insert(name, Value::Object(service));
        }
    }

    if dropped_container_names {
        notes.push("dropped `container_name`: berth 9 names containers per instance".to_owned());
    }
    notes.push(
        "berth 9 names containers and data volumes per instance, so existing 6/7 data volumes are not reused; restore data with `berth restore` after `berth up`"
            .to_owned(),
    );

    let swarm = match root.remove("swarm") {
        None => Vec::new(),
        Some(Value::Array(items)) => items,
        Some(other) => anyhow::bail!("`swarm` must be a list, found {other}"),
    };

    reject_remaining(&root, "top-level")?;
    Ok(RenderedProjectFile {
        schema_version: PROJECT_SCHEMA_VERSION,
        project,
        project_type,
        container_engine,
        services,
        swarm,
    })
}

fn convert_v8(
    mut root: Map<String, Value>,
    legacy_path: &Path,
    notes: &mut Vec<String>,
) -> Result<RenderedProjectFile> {
    let project = match root.remove("project") {
        Some(Value::String(project)) => project,
        Some(other) => anyhow::bail!("`project` must be a string, found {other}"),
        None => {
            let derived = derive_project_name(legacy_path)?;
            notes.push(format!(
                "no `project` was set; derived project name '{derived}' from the directory name"
            ));
            derived
        }
    };
    validate_project_name(&project).with_context(|| {
        format!("8.x project '{project}' cannot become the berth 9 project name")
    })?;

    if root.contains_key("workflows") {
        anyhow::bail!(
            "8.x `workflows` cannot be converted automatically; remove them and re-declare them for berth 9"
        );
    }

    let mut services = Map::new();
    if let Some(entries) = root.remove("services") {
        let Value::Object(entries) = entries else {
            anyhow::bail!("8.x `services` must be a mapping");
        };
        for (name, entry) in entries {
            let Value::Object(entry) = entry else {
                anyhow::bail!("8.x service '{name}' must be a mapping");
            };
            let mut converted = convert_v8_service(&name, entry)?;
            if drop_v8_shared_database_name(&mut converted) {
                notes.push(format!(
                    "service '{name}': dropped `database` — berth 9 names each instance's \
                     shared tenant itself and injects it as DB_DATABASE"
                ));
            }
            services.insert(name.clone(), Value::Object(converted));
        }
    }

    reject_remaining(&root, "8.x top-level")?;
    notes.push(
        "8.x data in daemon-owned volumes and shared databases is not reused; restore it with `berth restore` after `berth up`"
            .to_owned(),
    );
    Ok(RenderedProjectFile {
        schema_version: PROJECT_SCHEMA_VERSION,
        project,
        project_type: None,
        container_engine: None,
        services,
        swarm: Vec::new(),
    })
}

fn convert_v8_service(name: &str, mut entry: Map<String, Value>) -> Result<Map<String, Value>> {
    const RENAMED: [(&str, &str); 2] = [
        ("environment", "env"),
        ("environment_mapping", "env_mapping"),
    ];
    const COPIED: [&str; 6] = [
        "preset",
        "image",
        "depends_on",
        "php_extensions",
        "command",
        "database",
    ];

    let mut converted = Map::new();
    for key in COPIED {
        if let Some(value) = entry.remove(key) {
            converted.insert(key.to_owned(), value);
        }
    }
    for (from, to) in RENAMED {
        if let Some(value) = entry.remove(from) {
            converted.insert(to.to_owned(), value);
        }
    }
    let version = match entry.remove("version") {
        None => None,
        Some(Value::String(version)) => Some(version),
        Some(other) => {
            anyhow::bail!("8.x service '{name}' `version` must be a string, found {other}")
        }
    };
    if !converted.contains_key("image") {
        if let Some(Value::String(preset)) = converted.get("preset") {
            if let Some(image) = v8_catalog_image(preset, version.as_deref())? {
                converted.insert("image".to_owned(), Value::String(image));
            }
        }
    } else if version.is_some() {
        anyhow::bail!("8.x service '{name}' sets both `image` and `version`; keep one");
    }
    reject_remaining(&entry, &format!("8.x service '{name}'"))?;
    Ok(converted)
}

/// Mirrors the 7.0 default service name: the preset's declared name, else the
/// lowercased driver of the preset or of the service itself.
fn default_service_name(service: &Map<String, Value>) -> Result<String> {
    let driver = if let Some(preset) = service.get("preset") {
        let Value::String(preset) = preset else {
            anyhow::bail!("service `preset` must be a string, found {preset}");
        };
        let defaults = crate::config::presets::preset_defaults(preset)?;
        if let Some(name) = defaults.name {
            return Ok(name.to_owned());
        }
        defaults.driver
    } else {
        let driver = service.get("driver").ok_or_else(|| {
            anyhow::anyhow!("a service has neither `name`, `preset` nor `driver`")
        })?;
        serde_json::from_value::<crate::config::Driver>(driver.clone())
            .with_context(|| format!("invalid service `driver` {driver}"))?
    };
    Ok(format!("{driver:?}").to_lowercase())
}

/// Returns the image an 8.x preset resolved to, so a migrated preset-only
/// service keeps exactly the image it ran under 8.x instead of the different
/// 7.0 preset default. Mirrors 8.x `resolve_preset_artifact` (catalog
/// revision 2026-07-15.3). Worker presets inherit the application image.
fn v8_catalog_image(preset: &str, requested: Option<&str>) -> Result<Option<String>> {
    let preset = match preset {
        "pg" | "pgsql" => "postgres",
        "mssql" => "sqlserver",
        other => other,
    };
    if matches!(preset, "horizon" | "queue-worker" | "queue" | "scheduler") {
        return Ok(None);
    }
    let (default, pattern) = match preset {
        "mongodb" => ("8", "mongo:{v}"),
        "postgres" => ("18", "postgres:{v}"),
        "mysql" => ("8", "mysql:{v}"),
        "mariadb" => ("11", "mariadb:{v}"),
        "redis" => ("7", "redis:{v}-alpine"),
        "valkey" => ("8", "valkey/valkey:{v}"),
        "localstack" => ("4", "localstack/localstack:{v}"),
        "gotenberg" => ("8", "gotenberg/gotenberg:{v}"),
        "rabbitmq" => ("3", "rabbitmq:{v}-management"),
        "frankenphp" | "laravel" | "reverb" => ("8.5", "dunglas/frankenphp:php{v}"),
        "sqlserver" => (
            "2022",
            "mcr.microsoft.com/mssql/server:2022-CU25-ubuntu-22.04",
        ),
        "dragonfly" => ("1", "docker.dragonflydb.io/dragonflydb/dragonfly:v1.39.0"),
        "memcached" => ("1", "memcached:1.6-alpine"),
        "minio" => ("1", "minio/minio:RELEASE.2025-09-07T16-13-09Z"),
        "garage" => ("2", "dxflrs/garage:v2.3.0"),
        "rustfs" => ("1", "rustfs/rustfs:1.0.0-beta.2"),
        "opensearch" => ("3", "opensearchproject/opensearch:3.6.0"),
        "elasticsearch" => ("9", "docker.elastic.co/elasticsearch/elasticsearch:9.4.2"),
        "meilisearch" => ("1", "getmeili/meilisearch:v1.45.1"),
        "typesense" => ("0", "typesense/typesense:26.0"),
        "dusk" | "selenium" => ("4", "selenium/standalone-chromium:4.43.0-20260404"),
        "mailpit" => ("1", "axllent/mailpit:v1.30.0"),
        "soketi" => (
            "1",
            "quay.io/soketi/soketi:5d188786beaf683aca2115a6247dcdc15c29ac77-16-debian",
        ),
        other => anyhow::bail!("8.x preset '{other}' has no catalog entry to migrate"),
    };
    let version = requested.unwrap_or(default);
    if version.is_empty() {
        anyhow::bail!("8.x preset '{preset}' has an empty `version`");
    }
    if !pattern.contains("{v}") && version != default {
        anyhow::bail!("8.x preset '{preset}' supports only version {default}, found {version}");
    }
    Ok(Some(pattern.replace("{v}", version)))
}

fn reject_remaining(map: &Map<String, Value>, scope: &str) -> Result<()> {
    if map.is_empty() {
        return Ok(());
    }
    let keys = map
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!("{scope} fields have no berth 9 equivalent: {keys}")
}

fn derive_project_name(legacy_path: &Path) -> Result<String> {
    let dir_name = legacy_path
        .parent()
        .and_then(Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "cannot derive a project name from {}",
                legacy_path.display()
            )
        })?;
    let mut slug = String::with_capacity(dir_name.len());
    for character in dir_name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_matches('-').to_owned();
    slug.truncate(MAX_PROJECT_NAME_LEN);
    let slug = slug.trim_end_matches('-').to_owned();
    if slug.is_empty() {
        anyhow::bail!("directory name '{dir_name}' does not yield a project name; set `project`");
    }
    Ok(slug)
}
