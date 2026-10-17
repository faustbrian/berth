//! cli support profile names module.
//!
//! Contains cli support profile names logic used by Berth command workflows.

pub(crate) fn profile_names() -> Vec<&'static str> {
    vec!["full", "all", "infra", "data", "app", "web", "api"]
}

#[cfg(test)]
mod tests {
    use super::profile_names;

    #[test]
    fn profile_names_order_stable() {
        assert_eq!(
            profile_names(),
            vec!["full", "all", "infra", "data", "app", "web", "api"]
        );
    }
}
