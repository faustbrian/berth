//! An extra per-instance gateway route a service declares under `routes`.

/// One `routes` entry of a service, resolved for its instance.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ExtraRoute {
    /// The label from `.berth.yaml`.
    pub label: String,
    /// The env var that receives the route's URL.
    pub env: String,
    /// `{instance route label}-{label}.berth.localhost`.
    pub host: String,
}
