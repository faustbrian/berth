//! SQL client flavor selection used by serve image build/exec compatibility code.

use std::collections::HashMap;

pub(crate) const SQL_CLIENT_FLAVOR_ENV: &str = "BERTH_SQL_CLIENT_FLAVOR";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SqlClientFlavor {
    Mysql,
    Mariadb,
}

impl SqlClientFlavor {
    /// Returns the APT package name used to install the SQL client binary.
    #[must_use]
    pub(crate) fn apt_package(self) -> &'static str {
        match self {
            Self::Mysql => "default-mysql-client",
            Self::Mariadb => "mariadb-client",
        }
    }

    /// Returns normalized string form used in env/signature values.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Mysql => "mysql",
            Self::Mariadb => "mariadb",
        }
    }
}

/// Resolves preferred SQL client flavor from injected runtime env.
///
/// Unknown/missing values intentionally default to MySQL flavor.
#[must_use]
pub(crate) fn sql_client_flavor_from_injected_env(
    injected_env: &HashMap<String, String>,
) -> SqlClientFlavor {
    match injected_env
        .get(SQL_CLIENT_FLAVOR_ENV)
        .map(|value| value.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("mariadb") => SqlClientFlavor::Mariadb,
        _ => SqlClientFlavor::Mysql,
    }
}

#[cfg(test)]
mod tests {
    use super::{SqlClientFlavor, sql_client_flavor_from_injected_env};
    use std::collections::HashMap;

    #[test]
    fn defaults_to_mysql() {
        let injected = HashMap::new();
        assert_eq!(
            sql_client_flavor_from_injected_env(&injected),
            SqlClientFlavor::Mysql
        );
    }

    #[test]
    fn parses_mariadb() {
        let mut injected = HashMap::new();
        injected.insert("BERTH_SQL_CLIENT_FLAVOR".to_owned(), "mariadb".to_owned());
        assert_eq!(
            sql_client_flavor_from_injected_env(&injected),
            SqlClientFlavor::Mariadb
        );
    }
}
