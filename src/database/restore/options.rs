//! What a restore was asked to do.

use super::dump_file::DumpOrigin;

/// A restore's choices: whether to drop the database first, whether
/// `--gzip` was passed, and where the dump comes from (for its messages).
#[derive(Clone, Copy, Debug)]
pub(crate) struct RestoreOptions {
    /// `--reset`: drop and recreate the database before restoring.
    pub(crate) reset: bool,
    /// `--gzip` was passed. A `.gz` file name reads as gzip without it.
    pub(crate) gzip: bool,
    pub(crate) origin: DumpOrigin,
}
