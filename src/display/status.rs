//! display status module.
//!
//! Contains display status logic used by Berth command workflows.

use colored::Colorize;

use super::about_style::{AboutRow, print_section, print_section_with_title};
use crate::{config::ServiceConfig, docker};

/// Print a Laravel-style status overview for services.
pub fn print_status(services: &[&ServiceConfig]) {
    if services.is_empty() {
        let rows = [AboutRow::plain("Configured Services", "none")];
        print_section("Services", &rows);
        return;
    }

    let summary_rows = [AboutRow::plain(
        "Configured Services",
        services.len().to_string(),
    )];
    print_section("Services", &summary_rows);

    for service in services {
        print_service_section(service);
    }
}

fn print_service_section(service: &ServiceConfig) {
    let container_name = service
        .container_name()
        .unwrap_or_else(|_| "<unresolved>".to_owned());
    let raw_status =
        docker::inspect_status(&container_name).unwrap_or_else(|| "not created".to_owned());
    let status_value = render_status_value(&raw_status);

    let published_port =
        docker::inspect_host_port_binding(&container_name, service.resolved_container_port())
            .map(|(host, port)| format!("{host}:{port}"))
            .unwrap_or_else(|| format!("{}:{}", service.host, service.port));

    let mut rows = vec![
        AboutRow::plain("Kind", format!("{:?}", service.kind).to_lowercase()),
        AboutRow::plain("Driver", format!("{:?}", service.driver).to_lowercase()),
        AboutRow::plain("Image", service.image.as_str()),
        AboutRow::plain("Container", container_name),
        AboutRow::plain("Published Port", published_port),
        AboutRow::plain(
            "Internal Port",
            service.resolved_container_port().to_string(),
        ),
        AboutRow::plain("URL", service.connection_url()),
        AboutRow::colored("Status", raw_status, status_value),
    ];

    let domain_urls = service.resolved_domain_urls();
    if !domain_urls.is_empty() {
        rows.push(AboutRow::plain("Domain", domain_urls.join(", ")));
    }

    if let Some(env) = &service.env {
        rows.push(AboutRow::plain("Env Vars", env.len().to_string()));
    }

    if let Some(dependencies) = &service.depends_on {
        rows.push(AboutRow::plain("Depends On", dependencies.join(", ")));
    }

    rows.push(AboutRow::colored(
        "Octane",
        if service.octane {
            "enabled"
        } else {
            "disabled"
        },
        render_enabled_disabled(service.octane),
    ));

    let rendered_title = service.name.as_str().cyan().bold().to_string();
    print_section_with_title(&service.name, &rendered_title, &rows);
}

/// Renders status value for command execution.
fn render_status_value(status: &str) -> String {
    let normalized = status.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "running" => "running".green().bold().to_string(),
        "exited" | "dead" => normalized.red().bold().to_string(),
        "created" | "restarting" => normalized.yellow().bold().to_string(),
        "paused" => normalized.yellow().bold().to_string(),
        "dry-run" => normalized.cyan().bold().to_string(),
        "not created" => "not created".red().bold().to_string(),
        _ => normalized.normal().to_string(),
    }
}

/// Renders enabled disabled for command execution.
fn render_enabled_disabled(enabled: bool) -> String {
    if enabled {
        "enabled".green().bold().to_string()
    } else {
        "disabled".red().bold().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::render_enabled_disabled;
    use super::render_status_value;

    #[test]
    fn render_status_value_reformats_known_states() {
        let running = render_status_value("running");
        let exited = render_status_value("exited");
        let not_created = render_status_value("not created");
        let dead = render_status_value("dead");
        let created = render_status_value("created");
        let paused = render_status_value("paused");
        let dry_run = render_status_value("dry-run");
        let custom = render_status_value("custom");

        assert!(running.contains("running"));
        assert!(exited.contains("exited"));
        assert!(not_created.contains("not created"));
        assert!(dead.contains("dead"));
        assert!(created.contains("created"));
        assert!(paused.contains("paused"));
        assert!(dry_run.contains("dry-run"));
        assert!(custom.contains("custom"));
    }

    #[test]
    fn render_enabled_disabled_is_stable() {
        let enabled = render_enabled_disabled(true);
        let disabled = render_enabled_disabled(false);
        assert_ne!(enabled, disabled);
    }
}
