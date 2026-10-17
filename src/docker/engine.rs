//! Container runtime engine adapter metadata.

use crate::config::ContainerEngine;

/// Runtime health check specification used by doctor checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimeDiagnosticCheck {
    pub(crate) arg: &'static str,
    pub(crate) success_message: &'static str,
    pub(crate) failed_output_prefix: &'static str,
    pub(crate) failed_exec_prefix: &'static str,
}

/// Adapter contract for runtime-specific behavior.
pub(crate) trait RuntimeEngineAdapter {
    /// Returns engine kind.
    fn kind(&self) -> ContainerEngine;
    /// Returns command binary name.
    fn command_binary(&self) -> &'static str;
    /// Returns host-loopback alias usable from containers.
    fn host_gateway_alias(&self) -> &'static str;
    /// Returns optional `--add-host` mapping for host-loopback access.
    fn host_gateway_mapping(&self) -> Option<&'static str>;
    /// Returns diagnostic checks for `berth doctor`.
    fn diagnostics(&self) -> &'static [RuntimeDiagnosticCheck];
    /// Returns user-facing event source label.
    fn event_source_label(&self) -> &'static str;
    /// Returns log source key used by output event channels.
    fn log_source_key(&self) -> &'static str;
}

pub(super) fn adapter_for(kind: ContainerEngine) -> &'static dyn RuntimeEngineAdapter {
    match kind {
        ContainerEngine::Docker => &DockerEngineAdapter,
        ContainerEngine::Podman => &PodmanEngineAdapter,
    }
}

struct DockerEngineAdapter;
struct PodmanEngineAdapter;

const DOCKER_DIAGNOSTICS: [RuntimeDiagnosticCheck; 2] = [
    RuntimeDiagnosticCheck {
        arg: "--version",
        success_message: "Docker CLI available",
        failed_output_prefix: "Docker unavailable",
        failed_exec_prefix: "Docker unavailable",
    },
    RuntimeDiagnosticCheck {
        arg: "info",
        success_message: "Docker daemon reachable",
        failed_output_prefix: "Docker daemon not reachable",
        failed_exec_prefix: "Docker info failed",
    },
];

const PODMAN_DIAGNOSTICS: [RuntimeDiagnosticCheck; 2] = [
    RuntimeDiagnosticCheck {
        arg: "--version",
        success_message: "Podman CLI available",
        failed_output_prefix: "Podman unavailable",
        failed_exec_prefix: "Podman unavailable",
    },
    RuntimeDiagnosticCheck {
        arg: "info",
        success_message: "Podman runtime reachable",
        failed_output_prefix: "Podman runtime not reachable",
        failed_exec_prefix: "Podman info failed",
    },
];

impl RuntimeEngineAdapter for DockerEngineAdapter {
    fn kind(&self) -> ContainerEngine {
        ContainerEngine::Docker
    }

    fn command_binary(&self) -> &'static str {
        self.kind().command_binary()
    }

    fn host_gateway_alias(&self) -> &'static str {
        self.kind().host_gateway_alias()
    }

    fn host_gateway_mapping(&self) -> Option<&'static str> {
        self.kind().host_gateway_mapping()
    }

    fn diagnostics(&self) -> &'static [RuntimeDiagnosticCheck] {
        &DOCKER_DIAGNOSTICS
    }

    fn event_source_label(&self) -> &'static str {
        "Docker daemon"
    }

    fn log_source_key(&self) -> &'static str {
        "docker"
    }
}

impl RuntimeEngineAdapter for PodmanEngineAdapter {
    fn kind(&self) -> ContainerEngine {
        ContainerEngine::Podman
    }

    fn command_binary(&self) -> &'static str {
        self.kind().command_binary()
    }

    fn host_gateway_alias(&self) -> &'static str {
        self.kind().host_gateway_alias()
    }

    fn host_gateway_mapping(&self) -> Option<&'static str> {
        self.kind().host_gateway_mapping()
    }

    fn diagnostics(&self) -> &'static [RuntimeDiagnosticCheck] {
        &PODMAN_DIAGNOSTICS
    }

    fn event_source_label(&self) -> &'static str {
        "Podman runtime"
    }

    fn log_source_key(&self) -> &'static str {
        "podman"
    }
}

#[cfg(test)]
mod tests {
    use super::adapter_for;
    use crate::config::ContainerEngine;

    #[test]
    fn docker_adapter_exposes_expected_runtime_contract() {
        let adapter = adapter_for(ContainerEngine::Docker);
        assert_eq!(adapter.command_binary(), "docker");
        assert_eq!(adapter.host_gateway_alias(), "host.docker.internal");
        assert_eq!(
            adapter.host_gateway_mapping(),
            Some("host.docker.internal:host-gateway")
        );
        assert_eq!(adapter.diagnostics().len(), 2);
        assert_eq!(adapter.event_source_label(), "Docker daemon");
        assert_eq!(adapter.log_source_key(), "docker");
    }

    #[test]
    fn podman_adapter_exposes_expected_runtime_contract() {
        let adapter = adapter_for(ContainerEngine::Podman);
        assert_eq!(adapter.command_binary(), "podman");
        assert_eq!(adapter.host_gateway_alias(), "host.containers.internal");
        assert_eq!(adapter.host_gateway_mapping(), None);
        assert_eq!(adapter.diagnostics().len(), 2);
        assert_eq!(adapter.event_source_label(), "Podman runtime");
        assert_eq!(adapter.log_source_key(), "podman");
    }
}
