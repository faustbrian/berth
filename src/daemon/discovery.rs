//! Project discovery for daemon watch mode.

use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;

#[derive(Debug, Clone, Default)]
pub(crate) struct DiscoveryOptions {
    pub(crate) watch_dirs: Vec<PathBuf>,
    pub(crate) exclude_dirs: Vec<PathBuf>,
    pub(crate) max_projects: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct DiscoveryReport {
    pub(crate) projects: Vec<PathBuf>,
    pub(crate) invalid_configs: Vec<PathBuf>,
    pub(crate) duplicate_projects: Vec<PathBuf>,
    pub(crate) limited_projects: Vec<PathBuf>,
}

pub(crate) fn discover_projects(options: &DiscoveryOptions) -> Result<DiscoveryReport> {
    let mut report = DiscoveryReport::default();
    let mut seen = HashSet::new();
    let mut watch_dirs = options.watch_dirs.clone();
    watch_dirs.sort();

    for dir in &watch_dirs {
        discover_dir(dir, &mut seen, &mut report, options)?;
    }

    Ok(report)
}

fn discover_dir(
    dir: &Path,
    seen: &mut HashSet<PathBuf>,
    report: &mut DiscoveryReport,
    options: &DiscoveryOptions,
) -> Result<()> {
    if !dir.is_dir() || should_skip_dir(dir, options) {
        return Ok(());
    }

    // A directory whose project file cannot even be located (for example a
    // legacy-only stackctl project) is reported, never fatal for siblings.
    let located = match config::config_path_in_dir(dir) {
        Ok(located) => located,
        Err(_) => {
            report.invalid_configs.push(dir.to_path_buf());
            return Ok(());
        }
    };
    if located.is_some() {
        match config::load_config_with(config::LoadConfigPathOptions::new(None, Some(dir))) {
            Ok(_) => {
                let project_root = config::project_root_with(config::ProjectRootPathOptions::new(
                    None,
                    Some(dir),
                ))?;
                if report.max_projects_reached(options.max_projects) {
                    report.limited_projects.push(project_root);
                    return Ok(());
                }
                let canonical = fs::canonicalize(&project_root).unwrap_or(project_root.clone());
                if seen.insert(canonical) {
                    report.projects.push(project_root);
                } else {
                    report.duplicate_projects.push(project_root);
                }
                return Ok(());
            }
            Err(_) => report.invalid_configs.push(dir.to_path_buf()),
        }
    }

    let mut entries =
        fs::read_dir(dir)?.collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        if !entry.file_type()?.is_dir() {
            continue;
        }

        if should_skip_dir(&entry.path(), options) {
            continue;
        }

        discover_dir(&entry.path(), seen, report, options)?;
    }

    Ok(())
}

fn should_skip_dir(path: &Path, options: &DiscoveryOptions) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    if matches!(name, ".git" | "node_modules" | "vendor" | "target") {
        return true;
    }

    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    options.exclude_dirs.iter().any(|excluded| {
        let excluded = fs::canonicalize(excluded).unwrap_or_else(|_| excluded.clone());
        normalized == excluded || normalized.starts_with(&excluded)
    })
}

impl DiscoveryReport {
    fn max_projects_reached(&self, max_projects: Option<usize>) -> bool {
        max_projects.is_some_and(|limit| self.projects.len() >= limit)
    }
}

#[cfg(test)]
mod tests {
    use super::{DiscoveryOptions, discover_projects};
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let root = crate::test_support::temp_root().join(format!(
            "berth-daemon-discovery-{name}-{}",
            crate::test_support::unique_stamp()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn write_project(root: &PathBuf) {
        fs::create_dir_all(root).expect("create project root");
        crate::config::project_file::write_test_config_file(
            root.join(".berth.yaml"),
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        )
        .expect("write berth config");
    }

    #[test]
    fn discover_projects_skips_nested_configs_under_valid_project_root() {
        let root = temp_root("nested");
        let parent = root.join("parent");
        let child = parent.join("nested-child");
        write_project(&parent);
        write_project(&child);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            ..DiscoveryOptions::default()
        })
        .expect("discover projects");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.projects[0], parent);
    }

    #[test]
    fn discover_projects_deduplicates_overlapping_watch_dirs() {
        let root = temp_root("duplicate");
        let project = root.join("project");
        write_project(&project);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root.clone(), project.clone()],
            ..DiscoveryOptions::default()
        })
        .expect("discover");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.duplicate_projects, vec![project]);
    }

    #[test]
    fn discover_projects_reports_legacy_only_projects_and_keeps_valid_siblings() {
        let root = temp_root("legacy");
        let legacy = root.join("legacy");
        fs::create_dir_all(&legacy).expect("create legacy dir");
        fs::write(
            legacy.join(".stackctl.toml"),
            "container_prefix = \"old\"\n",
        )
        .expect("write legacy config");
        let valid = root.join("valid");
        write_project(&valid);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            ..DiscoveryOptions::default()
        })
        .expect("a legacy sibling must not abort discovery");
        assert_eq!(report.projects, vec![valid]);
        assert_eq!(report.invalid_configs, vec![legacy]);
    }

    #[test]
    fn discover_projects_reports_invalid_configs_and_keeps_valid_siblings() {
        let root = temp_root("invalid");
        let invalid = root.join("invalid");
        fs::create_dir_all(&invalid).expect("create invalid dir");
        fs::write(
            invalid.join(".berth.yaml"),
            "schema_version: 9\nproject: bad\nservices:\n  db:\n    preset: no-such-preset\n",
        )
        .expect("write invalid config");

        let valid = root.join("valid");
        write_project(&valid);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            ..DiscoveryOptions::default()
        })
        .expect("discover");
        assert_eq!(report.projects.len(), 1);
        assert_eq!(report.invalid_configs, vec![invalid]);
    }

    #[test]
    fn discover_projects_skips_excluded_directories() {
        let root = temp_root("exclude");
        let included = root.join("included");
        let excluded = root.join("excluded");
        write_project(&included);
        write_project(&excluded);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            exclude_dirs: vec![excluded.clone()],
            max_projects: None,
        })
        .expect("discover");

        assert_eq!(report.projects, vec![included]);
        assert!(report.duplicate_projects.is_empty());
        assert!(report.limited_projects.is_empty());
    }

    #[test]
    fn discover_projects_stops_after_max_project_limit() {
        let root = temp_root("limit");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        write_project(&alpha);
        write_project(&beta);

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            exclude_dirs: Vec::new(),
            max_projects: Some(1),
        })
        .expect("discover");

        assert_eq!(report.projects, vec![alpha]);
        assert_eq!(report.limited_projects, vec![beta]);
    }

    #[test]
    fn discover_projects_finds_yaml_configs() {
        let root = temp_root("yaml");
        let project = root.join("project");
        fs::create_dir_all(&project).expect("create project root");
        fs::write(
            project.join(".berth.yaml"),
            "schema_version: 9\nproject: demo\nproject_type: project\n",
        )
        .expect("write yaml config");

        let report = discover_projects(&DiscoveryOptions {
            watch_dirs: vec![root],
            ..DiscoveryOptions::default()
        })
        .expect("discover");

        assert_eq!(report.projects, vec![project]);
    }
}
