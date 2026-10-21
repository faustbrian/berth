//! docker prune dry-run preview helpers.

use anyhow::Result;

use super::docker_cmd::docker_output;
use crate::output::{self, LogLevel, Persistence};

/// Lists what `prune --all` would remove: the stopped containers matching
/// `filters`, except those another berth installation labeled.
pub(super) fn preview_global_prune_candidates(
    filters: &[String],
    installation: &str,
) -> Result<()> {
    let mut args = vec![
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        "status=exited".to_owned(),
        "--filter".to_owned(),
        "status=created".to_owned(),
        "--filter".to_owned(),
        "status=dead".to_owned(),
        "--format".to_owned(),
        format!(
            "{{{{.Names}}}}\t{}",
            crate::docker::label_template(crate::docker::LABEL_INSTALLATION)
        ),
    ];
    let filter_args: Vec<String> = filters
        .iter()
        .flat_map(|value| ["--filter".to_owned(), value.clone()])
        .collect();
    for item in &filter_args {
        args.push(item.clone());
    }

    let output = docker_output(&args, &crate::docker::runtime_command_error_context("ps"))?;
    if !output.status.success() {
        output::event(
            crate::docker::runtime_log_source_key(),
            LogLevel::Warn,
            "Dry-run preview could not list global prune candidates",
            Persistence::Persistent,
        );
        return Ok(());
    }

    let names: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (name, owner) = line.split_once('\t').unwrap_or((line, ""));
            let owner = owner.trim();
            let own = owner.is_empty() || owner == "<no value>" || owner == installation;
            (own && !name.trim().is_empty()).then(|| name.trim().to_owned())
        })
        .collect();
    if names.is_empty() {
        output::event(
            crate::docker::runtime_log_source_key(),
            LogLevel::Info,
            "Global prune dry-run preview found no stopped containers",
            Persistence::Persistent,
        );
        return Ok(());
    }

    output::event(
        crate::docker::runtime_log_source_key(),
        LogLevel::Info,
        &format!(
            "Global prune dry-run preview would remove {} stopped container(s): {}",
            names.len(),
            names.join(", ")
        ),
        Persistence::Persistent,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::preview_global_prune_candidates;
    use crate::docker;
    use std::fs;

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = crate::test_support::temp_root().join(format!(
            "berth-prune-preview-{}",
            crate::test_support::unique_stamp()
        ));
        fs::create_dir_all(&bin_dir).expect("temp dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        use std::io::Write;
        writeln!(file, "#!/bin/sh\n{}", script).expect("write script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let command = binary.to_string_lossy().to_string();
        let result = docker::with_docker_command(&command, test);
        fs::remove_dir_all(bin_dir).ok();
        result
    }

    #[test]
    fn preview_global_prune_candidates_reports_no_candidates() {
        with_fake_docker("printf ''\nexit 0", || {
            preview_global_prune_candidates(&[], "mine").expect("empty candidates");
        });
    }

    #[test]
    fn preview_global_prune_candidates_reports_candidates() {
        with_fake_docker("printf 'stopped-1\\nstopped-2\\n'\nexit 0", || {
            preview_global_prune_candidates(&["name=foo".to_owned()], "mine")
                .expect("candidate list");
        });
    }

    #[test]
    fn preview_global_prune_candidates_tolerates_list_failures() {
        with_fake_docker("printf 'oops' >&2\nexit 1", || {
            preview_global_prune_candidates(&[], "mine")
                .expect("non-zero preview output tolerated");
        });
    }
}
