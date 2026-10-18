//! config paths template module.
//!
//! Contains the starter `.berth.yaml` written by `berth init`.

use crate::config::domain_names::sanitize_project_slug;
use crate::config::project_file::schema::MAX_PROJECT_NAME_LEN;

/// Builds default `.berth.yaml` template content for the given project name.
#[must_use]
pub(super) fn default_config_template(project_name: &str) -> String {
    let mut slug = sanitize_project_slug(project_name);
    slug.truncate(MAX_PROJECT_NAME_LEN);
    let slug = slug.trim_end_matches('-');
    format!(
        r#"schema_version: 9
project: {slug}
project_type: project
container_engine: docker

services:
  app:
    preset: laravel
    # depends_on: [db, redis]
    #
    # Optional lifecycle hooks:
    # hook:
    #   - name: seed-dev-user
    #     phase: post_up
    #     on_error: fail
    #     run:
    #       type: exec
    #       argv: [php, artisan, db:seed, --class=DevUserSeeder]

  db:
    preset: mysql

  redis:
    preset: redis

  # Optional services:
  # s3:
  #   preset: rustfs
  # search:
  #   preset: meilisearch
  # mailhog:
  #   preset: mailhog
  # dusk:
  #   preset: dusk
  #   depends_on: [app]
"#
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::default_config_template;
    use crate::config::project_file::parse_project_file;

    #[test]
    fn default_template_is_valid_v9_yaml_and_preset_driven() {
        let raw = parse_project_file(&default_config_template("my app"), Path::new(".berth.yaml"))
            .expect("template parses as a berth project file");
        assert_eq!(raw.project_type, Some(crate::config::ProjectType::Project));
        let presets: Vec<_> = raw
            .service
            .iter()
            .map(|service| service.preset.as_deref().unwrap_or(""))
            .collect();

        assert_eq!(presets, vec!["laravel", "mysql", "redis"]);
    }

    #[test]
    fn default_template_uses_sanitized_bounded_project_slug() {
        let raw = parse_project_file(
            &default_config_template("Billing API for the Whole Company Group"),
            Path::new(".berth.yaml"),
        )
        .expect("template parses");
        let project = raw.container_prefix.as_deref().expect("project name");
        assert!(project.starts_with("billing-api"), "{project}");
        assert!(project.len() <= 24, "{project}");
    }

    #[test]
    fn default_template_falls_back_when_project_name_has_no_ascii_alnum() {
        let raw = parse_project_file(&default_config_template("___"), Path::new(".berth.yaml"))
            .expect("template parses");

        assert_eq!(raw.container_prefix.as_deref(), Some("my-app"));
    }
}
