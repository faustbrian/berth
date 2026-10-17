//! cli args commands operations docker ops attach module.
//!
//! Contains attach command args used by Berth command workflows.

use clap::Args;

#[derive(Args)]
pub(crate) struct AttachArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_stdin: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) sig_proxy: bool,
    #[arg(long)]
    pub(crate) detach_keys: Option<String>,
}

impl AttachArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn detach_keys(&self) -> Option<&str> {
        self.detach_keys.as_deref()
    }
}
