//! Go-template access to one label in `ps` and `volume ls` listings.

use crate::config::ContainerEngine;

/// Returns the `--format` template that prints label `key` of each listed
/// container or volume.
///
/// Docker's listings offer `{{.Label "key"}}` (their `.Labels` is a joined
/// string). Podman 4.9's have no `Label` function (`can't evaluate field
/// Label in type containers.psReporter`); their `.Labels` is a map.
pub(crate) fn label_template(key: &str) -> String {
    match super::container_engine() {
        ContainerEngine::Docker => format!("{{{{.Label \"{key}\"}}}}"),
        ContainerEngine::Podman => format!("{{{{index .Labels \"{key}\"}}}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::label_template;
    use crate::config::ContainerEngine;

    #[test]
    fn each_engine_reads_a_label_its_own_way() {
        assert_eq!(label_template("a.b"), r#"{{.Label "a.b"}}"#);
        crate::docker::with_container_engine(ContainerEngine::Podman, || {
            assert_eq!(label_template("a.b"), r#"{{index .Labels "a.b"}}"#);
        });
    }
}
