//! cli args commands operations docker ops cp module.
//!
//! Contains cp command args used by Berth command workflows.

use clap::Args;

#[derive(Args)]
pub(crate) struct CpArgs {
    /// Follow symlinks in SRC_PATH
    #[arg(short = 'L', long, default_value_t = false)]
    pub(crate) follow_link: bool,
    /// Copy all uid/gid information
    #[arg(short = 'a', long, default_value_t = false)]
    pub(crate) archive: bool,
    /// Source path (`service:/path` or `container:/path` or host path)
    pub(crate) source: String,
    /// Destination path (`service:/path` or `container:/path` or host path)
    pub(crate) destination: String,
}
