//! Berth's per-user state directory (`~/.berth`, or `BERTH_HOME`).
//!
//! Only leases, locks, credentials and generated gateway state live here;
//! Docker labels remain the source of truth for what berth owns.

mod installation;
mod lock;
mod paths;

pub(crate) use installation::installation_id;
pub(crate) use lock::HostLock;
pub(crate) use paths::private_dir;
