//! Every instance this installation owns, from Docker labels and the
//! instances' berth state (leases and credentials).

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;

use super::lease::{Lease, read_lease};

/// One of the instance's containers.
#[derive(Clone, Debug)]
pub(crate) struct InstanceContainer {
    pub(crate) name: String,
    pub(crate) state: String,
}

/// An instance this installation owns.
#[derive(Clone, Debug)]
pub(crate) struct InstanceRecord {
    pub(crate) project: String,
    pub(crate) instance: String,
    /// The owning checkout, from the lease or the containers' labels.
    pub(crate) root: Option<PathBuf>,
    pub(crate) lease: Option<Lease>,
    /// Why the lease could not be read; such an instance is never reaped.
    pub(crate) lease_error: Option<String>,
    pub(crate) containers: Vec<InstanceContainer>,
    /// The instance's networks (one per runtime environment that is up).
    pub(crate) networks: usize,
}

impl InstanceRecord {
    /// `running` (every container; an instance whose services are all
    /// tenants of shared servers has only its network), `partial`,
    /// `stopped` or `down` (berth state only).
    #[must_use]
    pub(crate) fn status(&self) -> &'static str {
        let running = self
            .containers
            .iter()
            .filter(|c| c.state == "running")
            .count();
        match (running, self.containers.len()) {
            (_, 0) if self.networks > 0 => "running",
            (_, 0) => "down",
            (running, total) if running == total => "running",
            (0, _) => "stopped",
            _ => "partial",
        }
    }

    /// Whether the owning checkout definitely no longer exists: the
    /// checkout is missing while its parent directory is there. A path that
    /// cannot be checked (permission denied, an I/O error), or whose parent is
    /// missing too (an unmounted external or network volume), counts as
    /// present: reaping deletes data, so doubt never does.
    #[must_use]
    pub(crate) fn owner_gone(&self) -> bool {
        self.root.as_ref().is_some_and(|root| {
            matches!(root.try_exists(), Ok(false))
                && root
                    .parent()
                    .is_some_and(|parent| matches!(parent.try_exists(), Ok(true)))
        })
    }
}

/// Lists every instance of this installation, sorted by project and ID.
///
/// # Errors
///
/// Returns an error when Docker cannot list containers or berth state
/// cannot be read.
pub(crate) fn list_instances() -> Result<Vec<InstanceRecord>> {
    let installation = crate::home::installation_id()?;
    let output = crate::docker::run_docker_output(
        &[
            "ps",
            "-a",
            "--filter",
            &format!("label=dev.berth.installation={installation}"),
            "--filter",
            "label=dev.berth.instance",
            "--format",
            "{{.Label \"dev.berth.project\"}}\t{{.Label \"dev.berth.instance\"}}\t{{.Label \"dev.berth.root\"}}\t{{.State}}\t{{.Names}}",
        ],
        "failed to list instance containers",
    )?;
    if !output.status.success() {
        anyhow::bail!(
            "failed to list instance containers: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut records: BTreeMap<(String, String), InstanceRecord> = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        let [project, instance, root, state, name] = fields[..] else {
            continue;
        };
        if project.is_empty() || instance.is_empty() {
            continue;
        }
        let record = records
            .entry((project.to_owned(), instance.to_owned()))
            .or_insert_with(|| empty(project, instance));
        if record.root.is_none() && !root.is_empty() {
            record.root = Some(PathBuf::from(root));
        }
        record.containers.push(InstanceContainer {
            name: name.to_owned(),
            state: state.to_owned(),
        });
    }
    let networks = crate::docker::run_docker_output(
        &[
            "network",
            "ls",
            "--filter",
            &format!("label=dev.berth.installation={installation}"),
            "--filter",
            "label=dev.berth.instance",
            "--format",
            "{{.Label \"dev.berth.project\"}}\t{{.Label \"dev.berth.instance\"}}\t{{.Name}}",
        ],
        "failed to list instance networks",
    )?;
    if !networks.status.success() {
        anyhow::bail!(
            "failed to list instance networks: {}",
            String::from_utf8_lossy(&networks.stderr).trim()
        );
    }
    for line in String::from_utf8_lossy(&networks.stdout).lines() {
        let mut fields = line.split('\t');
        let (Some(project), Some(instance), Some(name)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // A swarm endpoint network outlives the source's `down` while a
        // consumer is attached; it is not the instance running.
        if name.ends_with("_ep") {
            continue;
        }
        if project.is_empty() || instance.is_empty() {
            continue;
        }
        records
            .entry((project.to_owned(), instance.to_owned()))
            .or_insert_with(|| empty(project, instance))
            .networks += 1;
    }
    for (project, instance) in state_dirs()? {
        records
            .entry((project.clone(), instance.clone()))
            .or_insert_with(|| empty(&project, &instance));
    }
    // One unreadable lease is that instance's problem, not every listing's.
    for record in records.values_mut() {
        match read_lease(&record.project, &record.instance) {
            Ok(lease) => record.lease = lease,
            Err(error) => record.lease_error = Some(format!("{error:#}")),
        }
        if let Some(lease) = &record.lease {
            record.root = Some(lease.root.clone());
        } else if record.root.is_none() {
            record.root = super::root_record::recorded_root(&record.project, &record.instance);
        }
    }
    Ok(records.into_values().collect())
}

fn empty(project: &str, instance: &str) -> InstanceRecord {
    InstanceRecord {
        project: project.to_owned(),
        instance: instance.to_owned(),
        root: None,
        lease: None,
        lease_error: None,
        containers: Vec::new(),
        networks: 0,
    }
}

/// `(project, instance)` for every `~/.berth/instances/{project}/{instance}`.
fn state_dirs() -> Result<Vec<(String, String)>> {
    let root = crate::home::home_path(std::path::Path::new("instances"))?;
    let mut found = Vec::new();
    let projects = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", root.display()));
        }
    };
    for project in projects {
        let project = project.with_context(|| format!("failed to read {}", root.display()))?;
        if !project.file_type()?.is_dir() {
            continue;
        }
        let project_name = project.file_name().to_string_lossy().into_owned();
        for instance in std::fs::read_dir(project.path())? {
            let instance = instance?;
            if instance.file_type()?.is_dir() {
                found.push((
                    project_name.clone(),
                    instance.file_name().to_string_lossy().into_owned(),
                ));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::{InstanceContainer, InstanceRecord};

    fn record(states: &[&str]) -> InstanceRecord {
        InstanceRecord {
            project: "shop".to_owned(),
            instance: "a".to_owned(),
            root: None,
            lease: None,
            lease_error: None,
            networks: 0,
            containers: states
                .iter()
                .map(|state| InstanceContainer {
                    name: "c".to_owned(),
                    state: (*state).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn status_summarizes_the_containers() {
        assert_eq!(record(&[]).status(), "down");
        assert_eq!(record(&["running", "running"]).status(), "running");
        assert_eq!(record(&["running", "exited"]).status(), "partial");
        assert_eq!(record(&["exited"]).status(), "stopped");
        let mut shared_only = record(&[]);
        shared_only.networks = 1;
        assert_eq!(
            shared_only.status(),
            "running",
            "tenants of shared servers only"
        );
    }

    #[test]
    fn only_a_checkout_that_is_definitely_missing_is_gone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("berth-catalog-locked-{}", std::process::id()));
        let checkout = dir.join("a");
        std::fs::create_dir_all(&checkout).expect("checkout");
        let mut deleted = record(&[]);
        deleted.root = Some(dir.join("deleted"));
        assert!(deleted.owner_gone(), "a missing checkout is gone");
        let mut unmounted = record(&[]);
        unmounted.root = Some(dir.join("unmounted-volume").join("checkout"));
        assert!(
            !unmounted.owner_gone(),
            "a checkout whose parent is missing too may be on an unmounted volume"
        );
        let mut present = record(&[]);
        present.root = Some(checkout);
        assert!(!present.owner_gone());

        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("lock");
        let unreadable = std::fs::metadata(dir.join("a")).is_err();
        let probed = present.owner_gone();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("unlock");
        std::fs::remove_dir_all(&dir).expect("cleanup");
        // Root can stat anything; the check only means something without it.
        if unreadable {
            assert!(!probed, "a checkout that cannot be checked is not gone");
        }
    }
}
