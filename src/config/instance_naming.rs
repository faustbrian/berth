//! Applies an instance identity to every service of a loaded project.

use anyhow::Result;

use crate::config::{Config, Kind};
use crate::instance::InstanceIdentity;

/// Names every service `berth_{project}_{instance}_{service}`, namespaces the
/// named volumes of non-application services the same way, and records the
/// project, instance and root each service runs under.
///
/// `_` separates the parts because project names, instance IDs and service
/// names may contain `-` but never `_`, so a name always decodes uniquely.
///
/// # Errors
///
/// Returns an error when the project has no name, which only happens for
/// configs that were not loaded from a `.berth.yaml` file.
pub(crate) fn apply_instance_identity(
    config: &mut Config,
    identity: &InstanceIdentity,
) -> Result<()> {
    let project = config
        .container_prefix
        .clone()
        .ok_or_else(|| anyhow::anyhow!("a berth project needs a `project` name"))?;
    let namespace = format!("berth_{project}_{}", identity.id);
    for service in &mut config.service {
        service.resolved_container_name = Some(format!("{namespace}_{}", service.name));
        // Application mounts are resolved relative to the project root
        // (`storage:/app/storage` is a bind of `{root}/storage`), so only
        // other services' mounts can name Docker volumes.
        if service.kind != Kind::App {
            if let Some(volumes) = &mut service.volumes {
                for volume in volumes.iter_mut() {
                    *volume = namespace_named_volume(volume, &namespace);
                }
            }
        }
        service.runtime.project = Some(project.clone());
        service.runtime.instance = Some(identity.id.clone());
        service.runtime.root = Some(identity.root.clone());
        service.runtime.network = Some(namespace.clone());
    }
    Ok(())
}

/// Prefixes a named-volume source (`data:/var/lib/mysql`) with the instance
/// namespace so instances never share data. Only sources that are valid Docker
/// volume names (`[A-Za-z0-9][A-Za-z0-9_.-]*`) are named volumes; anything else
/// is a host path and is left unchanged.
fn namespace_named_volume(mapping: &str, namespace: &str) -> String {
    let Some((source, rest)) = mapping.split_once(':') else {
        return mapping.to_owned();
    };
    let named_volume = source
        .bytes()
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        && source
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'));
    if !named_volume {
        return mapping.to_owned();
    }
    format!("{namespace}_{source}:{rest}")
}

#[cfg(test)]
mod tests {
    use super::namespace_named_volume;

    #[test]
    fn named_volumes_are_namespaced_and_host_paths_are_not() {
        assert_eq!(
            namespace_named_volume("dbdata:/var/lib/mysql", "berth_api_agent-1-97fe08"),
            "berth_api_agent-1-97fe08_dbdata:/var/lib/mysql"
        );
        assert_eq!(
            namespace_named_volume("cache:/data:ro", "berth_api_x-1"),
            "berth_api_x-1_cache:/data:ro"
        );
        for unchanged in [
            "./storage:/app/storage",
            "/srv/data:/data",
            "~/x:/x",
            "docker/php.ini:/usr/local/etc/php/conf.d/zz.ini",
            "/anonymous",
        ] {
            assert_eq!(
                namespace_named_volume(unchanged, "berth_api_x-1"),
                unchanged
            );
        }
    }
}
