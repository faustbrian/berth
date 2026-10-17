//! cli args commands app share module.
//!
//! Contains cli args for `berth share` workflows.

use clap::{Args, Subcommand};

use crate::cli::args::ShareProviderArg;

#[derive(Clone, Copy)]
pub(crate) struct ShareProviderSelectionArgs {
    pub(crate) provider: Option<ShareProviderArg>,
    pub(crate) cloudflare: bool,
    pub(crate) expose: bool,
    pub(crate) tailscale: bool,
}

#[derive(Args)]
pub(crate) struct ShareArgs {
    #[command(subcommand)]
    pub(crate) command: ShareCommands,
}

#[derive(Subcommand)]
pub(crate) enum ShareCommands {
    /// Start a sharing tunnel for an app service
    Start(ShareStartArgs),
    /// Show tracked sharing tunnel sessions
    Status(ShareStatusArgs),
    /// Stop tracked sharing tunnel sessions
    Stop(ShareStopArgs),
}

#[derive(Args)]
pub(crate) struct ShareStartArgs {
    /// App service name to expose
    #[arg(long)]
    pub(crate) service: Option<String>,
    /// Share provider runtime
    #[arg(
        long,
        value_enum,
        conflicts_with_all = ["cloudflare", "expose", "tailscale"],
        required_unless_present_any = ["cloudflare", "expose", "tailscale"]
    )]
    pub(crate) provider: Option<ShareProviderArg>,
    /// Shorthand for `--provider cloudflare`
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["provider", "expose", "tailscale"]
    )]
    pub(crate) cloudflare: bool,
    /// Shorthand for `--provider expose`
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["provider", "cloudflare", "tailscale"]
    )]
    pub(crate) expose: bool,
    /// Shorthand for `--provider tailscale`
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["provider", "cloudflare", "expose"]
    )]
    pub(crate) tailscale: bool,
    /// Keep process running in background and persist session metadata
    #[arg(long, default_value_t = false)]
    pub(crate) detached: bool,
    /// Max seconds to wait for public URL verification before failing
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    /// Print machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl ShareStartArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn provider_selection(&self) -> ShareProviderSelectionArgs {
        share_provider_selection(self.provider, self.cloudflare, self.expose, self.tailscale)
    }
}

#[derive(Args)]
pub(crate) struct ShareStatusArgs {
    /// Filter by app service name
    #[arg(long)]
    pub(crate) service: Option<String>,
    /// Filter by provider
    #[arg(long, value_enum, conflicts_with_all = ["cloudflare", "expose", "tailscale"])]
    pub(crate) provider: Option<ShareProviderArg>,
    /// Shorthand for `--provider cloudflare`
    #[arg(long, default_value_t = false, conflicts_with_all = ["expose", "tailscale"])]
    pub(crate) cloudflare: bool,
    /// Shorthand for `--provider expose`
    #[arg(long, default_value_t = false, conflicts_with_all = ["cloudflare", "tailscale"])]
    pub(crate) expose: bool,
    /// Shorthand for `--provider tailscale`
    #[arg(long, default_value_t = false, conflicts_with_all = ["cloudflare", "expose"])]
    pub(crate) tailscale: bool,
    /// Print machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl ShareStatusArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn provider_selection(&self) -> ShareProviderSelectionArgs {
        share_provider_selection(self.provider, self.cloudflare, self.expose, self.tailscale)
    }
}

#[derive(Args)]
pub(crate) struct ShareStopArgs {
    /// Filter by app service name
    #[arg(long)]
    pub(crate) service: Option<String>,
    /// Filter by provider
    #[arg(long, value_enum, conflicts_with_all = ["cloudflare", "expose", "tailscale"])]
    pub(crate) provider: Option<ShareProviderArg>,
    /// Shorthand for `--provider cloudflare`
    #[arg(long, default_value_t = false, conflicts_with_all = ["expose", "tailscale"])]
    pub(crate) cloudflare: bool,
    /// Shorthand for `--provider expose`
    #[arg(long, default_value_t = false, conflicts_with_all = ["cloudflare", "tailscale"])]
    pub(crate) expose: bool,
    /// Shorthand for `--provider tailscale`
    #[arg(long, default_value_t = false, conflicts_with_all = ["cloudflare", "expose"])]
    pub(crate) tailscale: bool,
    /// Stop all tracked sessions
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    /// Print machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl ShareStopArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn provider_selection(&self) -> ShareProviderSelectionArgs {
        share_provider_selection(self.provider, self.cloudflare, self.expose, self.tailscale)
    }
}

fn share_provider_selection(
    provider: Option<ShareProviderArg>,
    cloudflare: bool,
    expose: bool,
    tailscale: bool,
) -> ShareProviderSelectionArgs {
    ShareProviderSelectionArgs {
        provider,
        cloudflare,
        expose,
        tailscale,
    }
}
