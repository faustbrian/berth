//! Shared docker image pull command helper.

use anyhow::Result;

use super::run_docker_status;

/// Executes `docker pull <image>` and fails when docker returns non-zero.
pub(crate) fn docker_pull(image: &str, context: &str, failure_message: &str) -> Result<()> {
    let status = run_docker_status(&["pull", image], context)?;

    if status.success() {
        return Ok(());
    }

    anyhow::bail!("{failure_message}");
}

#[cfg(test)]
mod tests {
    use crate::docker;
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    fn with_fake_docker<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = env::temp_dir().join(format!(
            "berth-image-pull-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("write fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("script");
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod");
        }

        let binary = binary.to_string_lossy().to_string();
        let result =
            docker::with_dry_run_state(false, || docker::with_docker_command(&binary, test));
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn docker_pull_succeeds_when_docker_succeeds() {
        with_fake_docker("exit 0", || {
            super::docker_pull("redis:7", "pull image", "pull failed").expect("image pull");
        });
    }

    #[test]
    fn docker_pull_returns_error_when_docker_fails() {
        with_fake_docker("exit 1", || {
            let result = super::docker_pull("redis:7", "pull image", "pull failed");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("pull failed"));
        });
    }
}
