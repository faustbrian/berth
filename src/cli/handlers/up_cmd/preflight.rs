//! Preflight preparation for `up` command runtime context.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;

use crate::{cli, config, swarm};

pub(super) struct PreparedUpContext {
    pub(super) workspace_root: PathBuf,
    pub(super) project_dependency_env: HashMap<String, String>,
    pub(super) env_path: Option<PathBuf>,
}

pub(super) struct PrepareUpContextOptions<'a> {
    pub(super) repro: bool,
    pub(super) env_output: bool,
    pub(super) config_path: Option<&'a Path>,
    pub(super) project_root: Option<&'a Path>,
    pub(super) include_project_deps: bool,
    pub(super) quiet: bool,
    pub(super) no_color: bool,
    pub(super) dry_run: bool,
    pub(super) runtime_env: Option<&'a str>,
}

pub(super) fn prepare_up_context(
    config: &mut config::Config,
    options: PrepareUpContextOptions<'_>,
) -> Result<PreparedUpContext> {
    if options.repro {
        super::options::validate_repro_flags(options.env_output)?;
        config::verify_lockfile_with(
            config,
            config::ProjectRootPathOptions::new(options.config_path, options.project_root),
        )?;
    }

    let workspace_root =
        cli::support::workspace_with_project_deps(cli::support::WorkspaceWithProjectDepsOptions {
            operation: "up",
            config_path: options.config_path,
            project_root: options.project_root,
            include_project_deps: options.include_project_deps,
            quiet: options.quiet,
            no_color: options.no_color,
            dry_run: options.dry_run,
            runtime_env: options.runtime_env,
            force_down_deps: false,
        })?;

    // `--no-deps` only skips starting the dependencies; the app still gets
    // their endpoints (as `berth env` and every other command do), and the
    // network join fails precisely when a source is not up.
    let project_dependency_env =
        swarm::resolve_project_dependency_injected_env(&workspace_root, options.runtime_env)?;
    let env_path = cli::support::env_output_path(
        options.env_output,
        options.config_path,
        options.project_root,
        options.runtime_env,
    )?;

    Ok(PreparedUpContext {
        workspace_root,
        project_dependency_env,
        env_path,
    })
}

#[cfg(test)]
mod tests {
    use super::{PrepareUpContextOptions, prepare_up_context};
    use crate::config::{Config, ProjectType};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn config() -> Config {
        Config {
            schema_version: 1,
            project_type: ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: Vec::new(),
            swarm: Vec::new(),
        }
    }

    #[test]
    fn prepare_up_context_resolves_injected_env_even_without_dependencies() -> anyhow::Result<()> {
        let root = std::env::temp_dir().join(format!(
            "berth-up-preflight-no-deps-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root)?;
        crate::config::project_file::write_test_config_file(
            root.join(".berth.yaml"),
            r#"
project_type = "project"
[[swarm]]
name = "api"
root = "./"

[[swarm.inject_env]]
env = "LOCATION_API_BASE_URL"
from = "location"
value = ":base_url"
"#,
        )?;

        let mut cfg = config();
        let result = prepare_up_context(
            &mut cfg,
            PrepareUpContextOptions {
                repro: false,
                env_output: false,
                config_path: None,
                project_root: Some(&root),
                include_project_deps: false,
                quiet: true,
                no_color: true,
                dry_run: true,
                runtime_env: None,
            },
        );

        fs::remove_dir_all(&root)?;
        // `--no-deps` skips starting dependencies, not resolving their
        // endpoints: the unknown source is reported.
        let error = result.err().expect("the unknown source is an error");
        assert!(
            format!("{error:#}").contains("unknown target 'location'"),
            "{error:#}"
        );
        Ok(())
    }
}
