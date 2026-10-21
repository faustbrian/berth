//! docker inspect label module.
//!
//! Contains docker inspect label lookup logic used by Berth command workflows.

use crate::docker::is_dry_run;

use super::command::docker_inspect_format;

#[must_use]
pub(super) fn inspect_label(container_name: &str, key: &str) -> Option<String> {
    if is_dry_run() {
        return Some(String::new());
    }

    let template = format!("{{{{index .Config.Labels \"{key}\"}}}}");
    let output = docker_inspect_format(container_name, &template)?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use super::inspect_label;
    use crate::docker::with_docker_command;

    #[test]
    fn inspect_label_returns_empty_on_dry_run() {
        let label = crate::docker::with_dry_run_lock(|| inspect_label("web", "berth.managed"));
        assert_eq!(label, Some(String::new()));
    }

    #[test]
    fn inspect_label_reads_value_from_inspect_output() {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-inspect-label-{}",
            crate::test_support::unique_stamp()
        ));
        fs::create_dir_all(&bin_dir).expect("fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("write fake docker");
        writeln!(file, "#!/bin/sh\nprintf 'true'").expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary)
                .expect("binary permissions")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let result = with_docker_command(&binary.to_string_lossy(), || {
            inspect_label("web", "berth.managed")
        });
        fs::remove_dir_all(&bin_dir).ok();
        assert_eq!(result, Some("true".to_owned()));
    }
}
