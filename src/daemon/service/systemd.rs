//! systemd user-unit rendering for daemon watch services.

use anyhow::Result;
use std::path::Path;

use super::{DaemonServiceDefinition, ServiceContext, ServiceManager};

pub(super) fn definition(
    context: &ServiceContext,
    label: &str,
    path: &Path,
) -> Result<DaemonServiceDefinition> {
    Ok(DaemonServiceDefinition {
        manager: ServiceManager::SystemdUser,
        label: label.to_owned(),
        path: path.to_path_buf(),
        contents: render_unit(context),
    })
}

fn render_unit(context: &ServiceContext) -> String {
    let exec_start = std::iter::once(&context.binary)
        .chain(context.args.iter())
        .map(|value| systemd_quote(value))
        .collect::<Vec<_>>()
        .join(" ");

    format!(
        "[Unit]\nDescription=Berth daemon watch service\nAfter=default.target\n\n\
[Service]\nType=simple\nExecStart={exec_start}\nRestart=always\n\
RestartSec=5\nStandardOutput=append:{stdout_path}\n\
StandardError=append:{stderr_path}\n\n[Install]\nWantedBy=default.target\n",
        exec_start = exec_start,
        stdout_path = context.stdout_path.to_string_lossy(),
        stderr_path = context.stderr_path.to_string_lossy(),
    )
}

fn systemd_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}
