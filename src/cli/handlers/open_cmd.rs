//! cli handlers open cmd module.
//!
//! Contains cli handlers open cmd logic used by Berth command workflows.

use anyhow::Result;

use crate::{cli, config};

mod database_url;
mod reporting;
use database_url::database_runtime_url;
use reporting::{render_open_json, render_open_text};

pub(crate) struct HandleOpenOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) all: bool,
    pub(crate) health_path: Option<&'a str>,
    pub(crate) no_browser: bool,
    pub(crate) non_interactive: bool,
    pub(crate) database: bool,
    pub(crate) json: bool,
}

pub(crate) fn handle_open(config: &config::Config, options: HandleOpenOptions<'_>) -> Result<()> {
    if options.database {
        if options.json {
            return render_open_json(config, &[], options.health_path);
        }
        return render_open_text(config, &[], options.health_path, true, true);
    }

    let targets = resolve_open_targets(
        config,
        options.service,
        options.kind,
        options.profile,
        options.all,
    )?;

    if targets.is_empty() {
        anyhow::bail!("no app services configured");
    }

    if options.json {
        render_open_json(config, &targets, options.health_path)?;
    } else {
        render_open_text(
            config,
            &targets,
            options.health_path,
            options.no_browser || options.non_interactive,
            options.database,
        )?;
    }

    Ok(())
}

fn resolve_open_targets<'a>(
    config: &'a config::Config,
    service: Option<&'a str>,
    kind: Option<config::Kind>,
    profile: Option<&'a str>,
    all: bool,
) -> Result<Vec<&'a config::ServiceConfig>> {
    if all {
        return Ok(cli::support::app_services(config));
    }
    if profile.is_some() || kind.is_some() {
        let selected = cli::support::selected_services_with_filters(
            config,
            service,
            &[],
            kind,
            None,
            profile,
        )?;
        return Ok(selected
            .into_iter()
            .filter(|svc| svc.kind == config::Kind::App)
            .collect());
    }
    Ok(vec![config::resolve_app_service(config, service)?])
}

#[cfg(test)]
mod tests {
    use super::database_url::build_database_url_with_binding;
    use super::{HandleOpenOptions, handle_open};
    use anyhow::Result;

    #[test]
    fn runtime_binding_overrides_configured_database_port() -> Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "127.0.0.1".to_owned();
        service.port = 33060;
        service.username = Some("laravel".to_owned());
        service.password = Some("laravel".to_owned());
        service.database = Some("laravel".to_owned());

        let url = build_database_url_with_binding(&service, Some(("127.0.0.1".to_owned(), 49123)));
        assert_eq!(url, "mysql://laravel:laravel@127.0.0.1:49123/laravel");
        Ok(())
    }

    #[test]
    fn runtime_binding_falls_back_to_configured_host_for_unspecified_any_host() -> Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "127.0.0.1".to_owned();
        service.port = 33060;
        service.username = Some("laravel".to_owned());
        service.password = Some("laravel".to_owned());
        service.database = Some("laravel".to_owned());

        let url = build_database_url_with_binding(&service, Some(("0.0.0.0".to_owned(), 49123)));
        assert_eq!(url, "mysql://laravel:laravel@127.0.0.1:49123/laravel");
        Ok(())
    }

    #[test]
    fn runtime_binding_falls_back_for_unspecified_ipv6_any_host() -> Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "localhost".to_owned();
        service.port = 33060;
        service.username = Some("laravel".to_owned());
        service.password = Some("laravel".to_owned());
        service.database = Some("laravel".to_owned());

        let url = build_database_url_with_binding(&service, Some(("[::]".to_owned(), 49123)));
        assert_eq!(url, "mysql://laravel:laravel@localhost:49123/laravel");
        Ok(())
    }

    #[test]
    fn database_mode_does_not_require_app_services() -> Result<()> {
        let config = crate::config::Config {
            schema_version: 1,
            project_type: crate::config::ProjectType::Project,
            container_prefix: None,
            domain_strategy: None,
            service: vec![crate::config::preset_preview("postgres")?],
            swarm: Vec::new(),
        };

        handle_open(
            &config,
            HandleOpenOptions {
                service: None,
                kind: None,
                profile: None,
                all: false,
                health_path: None,
                no_browser: false,
                non_interactive: false,
                database: true,
                json: true,
            },
        )?;

        Ok(())
    }
}
