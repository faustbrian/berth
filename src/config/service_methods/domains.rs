//! config service methods domains module.
//!
//! Contains config service methods domains logic used by Berth command workflows.

use std::collections::HashSet;

use super::ServiceConfig;

impl ServiceConfig {
    #[must_use]
    pub fn resolved_domains(&self) -> Vec<&str> {
        let mut domains = Vec::new();
        let mut seen = HashSet::new();

        if let Some(primary) = self.domain.as_deref() {
            let trimmed = primary.trim();
            if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
                domains.push(trimmed);
            }
        }

        if let Some(additional) = &self.domains {
            for domain in additional {
                let trimmed = domain.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if seen.insert(trimmed.to_ascii_lowercase()) {
                    domains.push(trimmed);
                }
            }
        }

        if domains.is_empty()
            && let Some(generated) = self.resolved_domain.as_deref()
        {
            let trimmed = generated.trim();
            if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
                domains.push(trimmed);
            }
        }

        domains
    }

    #[must_use]
    pub fn primary_domain(&self) -> Option<&str> {
        self.resolved_domains().into_iter().next()
    }

    #[must_use]
    pub fn resolved_domain_urls(&self) -> Vec<String> {
        let scheme = self.scheme.as_deref().unwrap_or("https");
        self.resolved_domains()
            .into_iter()
            .map(|domain| {
                if domain.contains("://") {
                    domain.to_owned()
                } else {
                    format!("{scheme}://{domain}")
                }
            })
            .collect()
    }
}
