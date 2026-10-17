//! Names of shared servers and tenants.

use sha2::{Digest, Sha256};

use crate::config::SharedEngine;

/// Longest tenant name berth creates. PostgreSQL truncates identifiers at 63
/// bytes; ParaTest appends `_test_{token}` to the database name, so 11 bytes
/// stay free for it.
const MAX_TENANT_LEN: usize = 52;
/// Longest readable `{project}__{instance}` base before it is hashed.
const MAX_BASE_LEN: usize = 40;

fn hex(input: &str, len: usize) -> String {
    let digest = Sha256::digest(input.as_bytes());
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()[..len]
        .to_owned()
}

/// The compatibility fingerprint a shared server is keyed by: the engine,
/// image and server command. Instances, projects, service names and ports
/// never enter it, so every compatible instance shares one server.
#[must_use]
pub(crate) fn fingerprint(engine: SharedEngine, image: &str, command: Option<&[String]>) -> String {
    let command = command
        .map(|command| command.join("\u{1f}"))
        .unwrap_or_default();
    hex(
        &format!("berth-shared-v1\0{}\0{image}\0{command}", engine.as_str()),
        64,
    )
}

/// Container name of the shared server for `fingerprint` in an installation.
#[must_use]
pub(crate) fn server_name(engine: SharedEngine, installation: &str, fingerprint: &str) -> String {
    format!(
        "berth_shared_{}_{}",
        engine.as_str(),
        hex(&format!("{installation}\0{fingerprint}"), 12)
    )
}

/// The instance's tenant base, `{project}__{instance}` (hashed when long).
/// `__` cannot occur inside a project or instance name, so two instances
/// never share a base and a base never prefixes another instance's names.
#[must_use]
pub(crate) fn tenant_base(project: &str, instance: &str) -> String {
    let base = format!("{project}__{instance}");
    if base.len() <= MAX_BASE_LEN {
        return base;
    }
    let short: String = project.chars().take(12).collect();
    format!("{short}__{}", hex(&format!("{project}\0{instance}"), 12))
}

/// The tenant (database and role) of one service in an instance's runtime
/// environment: `{base}__{service}`, plus `__{env}` for a non-default
/// environment. A tail that would not fit is hashed; the base stays, so
/// purges still find every tenant of the instance.
#[must_use]
pub(crate) fn tenant_name(
    project: &str,
    instance: &str,
    service: &str,
    env: Option<&str>,
) -> String {
    let base = tenant_base(project, instance);
    let tail = match env {
        Some(env) => format!("{service}__{env}"),
        None => service.to_owned(),
    };
    let full = format!("{base}__{tail}");
    if full.len() <= MAX_TENANT_LEN {
        return full;
    }
    format!("{base}__{}", hex(&tail, 10))
}

/// Whether `name` is one of the instance's tenants or the test databases
/// ParaTest creates for them.
#[must_use]
pub(crate) fn belongs_to_instance(name: &str, project: &str, instance: &str) -> bool {
    let base = tenant_base(project, instance);
    name == base
        || name
            .strip_prefix(&base)
            .is_some_and(|rest| rest.starts_with('_'))
}

#[cfg(test)]
mod tests {
    use super::{belongs_to_instance, fingerprint, server_name, tenant_name};
    use crate::config::SharedEngine;

    #[test]
    fn the_fingerprint_depends_on_the_server_only() {
        let a = fingerprint(SharedEngine::Postgres, "postgres:17", None);
        assert_eq!(a, fingerprint(SharedEngine::Postgres, "postgres:17", None));
        assert_ne!(a, fingerprint(SharedEngine::Postgres, "postgres:18", None));
        assert_ne!(
            a,
            fingerprint(
                SharedEngine::Postgres,
                "postgres:17",
                Some(&["postgres".to_owned(), "-c".to_owned()])
            )
        );
        assert!(
            server_name(SharedEngine::Postgres, "inst", &a).starts_with("berth_shared_postgres_")
        );
        assert_ne!(
            server_name(SharedEngine::Postgres, "one", &a),
            server_name(SharedEngine::Postgres, "two", &a)
        );
    }

    #[test]
    fn tenants_are_bounded_unique_and_never_claim_a_neighbour() {
        let long_project = "p".repeat(24);
        let long_instance = format!("{}-abc123", "i".repeat(16));
        let env = "testing-pool-0123456789abcdef-32";
        for (project, instance) in [
            ("shop", "a"),
            (long_project.as_str(), long_instance.as_str()),
        ] {
            for env in [None, Some("staging"), Some(env)] {
                let name = tenant_name(project, instance, "db", env);
                assert!(name.len() <= 52, "{name}");
                assert!(belongs_to_instance(&name, project, instance), "{name}");
                assert!(belongs_to_instance(
                    &format!("{name}_test_7"),
                    project,
                    instance
                ));
            }
        }
        for (mine, other) in [
            ("a", "a-test-1"),
            ("foo-abc123", "foo-abc123-test"),
            ("agent", "agent-2"),
        ] {
            let theirs = tenant_name("shop", other, "db", None);
            assert!(
                !belongs_to_instance(&theirs, "shop", mine),
                "{mine} claims {theirs}"
            );
            assert!(!belongs_to_instance(
                &format!("{theirs}_test_1"),
                "shop",
                mine
            ));
        }
        assert_ne!(
            tenant_name("a-b", "c", "db", None),
            tenant_name("a", "b-c", "db", None)
        );
        assert_ne!(
            tenant_name("shop", "a", "primary", None),
            tenant_name("shop", "a", "analytics", None)
        );
    }
}
