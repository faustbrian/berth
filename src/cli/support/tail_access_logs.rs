//! cli support tail access logs module.
//!
//! Shows the gateway's access log, which Caddy writes to the gateway
//! container's stderr.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};

pub(crate) fn tail_access_logs(follow: bool, tail: Option<u64>) -> Result<()> {
    if crate::docker::is_dry_run() {
        let args = access_log_args("<berth-gateway>", follow, tail);
        output::event(
            "logs",
            LogLevel::Info,
            &format!("[dry-run] {}", crate::docker::runtime_command_text(&args)),
            Persistence::Transient,
        );
        return Ok(());
    }
    let args = access_log_args(&crate::gateway::gateway_name()?, follow, tail);
    let status =
        crate::docker::run_docker_status_owned(&args, "failed to read gateway access logs")?;
    if !status.success() {
        anyhow::bail!("the berth gateway is not running; start an app with `berth up` first");
    }
    Ok(())
}

fn access_log_args(gateway: &str, follow: bool, tail: Option<u64>) -> Vec<String> {
    let mut args = vec![
        "logs".to_owned(),
        "--tail".to_owned(),
        tail.unwrap_or(100).to_string(),
    ];
    if follow {
        args.push("-f".to_owned());
    }
    args.push(gateway.to_owned());
    args
}

#[cfg(test)]
mod tests {
    use super::access_log_args;

    #[test]
    fn access_logs_come_from_the_gateway_container() {
        assert_eq!(
            access_log_args("berth_gateway_abc", true, Some(25)),
            ["logs", "--tail", "25", "-f", "berth_gateway_abc"]
        );
        assert_eq!(
            access_log_args("berth_gateway_abc", false, None),
            ["logs", "--tail", "100", "berth_gateway_abc"]
        );
    }
}
