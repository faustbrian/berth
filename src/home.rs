//! Berth's per-user state directory (`~/.berth`, or `BERTH_HOME`).
//!
//! Only leases, locks, credentials and generated gateway state live here;
//! Docker labels remain the source of truth for what berth owns.

mod app_key;
mod installation;
mod lock;
mod paths;
mod same_file;
mod secret_file;

pub(crate) use app_key::app_key;
pub(crate) use installation::installation_id;
pub(crate) use lock::HostLock;
pub(crate) use paths::{home_path, private_dir, remove_instance_state};
pub(crate) use same_file::same_file;
pub(crate) use secret_file::{is_hex_secret, random_hex_secret, read_or_create};
