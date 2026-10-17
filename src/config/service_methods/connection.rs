//! config service methods connection module.
//!
//! Contains config service methods connection logic used by Berth command workflows.

use super::{Driver, ServiceConfig};

impl ServiceConfig {
    /// Connection URL for clients.
    #[must_use]
    pub fn connection_url(&self) -> String {
        let scheme = self.scheme();
        let host = format_host_for_url(&self.host);
        let port = self.port;

        match self.driver {
            Driver::Mongodb => {
                let user = encode_url_component(self.username.as_deref().unwrap_or("root"));
                let db = encode_url_component(self.database.as_deref().unwrap_or("app"));
                let password = encode_url_component(self.password.as_deref().unwrap_or(""));
                if password.is_empty() {
                    format!("mongodb://{user}@{host}:{port}/{db}")
                } else {
                    format!("mongodb://{user}:{password}@{host}:{port}/{db}")
                }
            }
            Driver::Postgres | Driver::Mysql | Driver::Sqlserver => {
                let user = encode_url_component(self.username.as_deref().unwrap_or("root"));
                let db = encode_url_component(self.database.as_deref().unwrap_or("app"));
                let password = encode_url_component(self.password.as_deref().unwrap_or(""));
                let db_scheme = if matches!(self.driver, Driver::Postgres) {
                    "postgresql"
                } else if matches!(self.driver, Driver::Sqlserver) {
                    "sqlsrv"
                } else {
                    "mysql"
                };
                if password.is_empty() {
                    format!("{db_scheme}://{user}@{host}:{port}/{db}")
                } else {
                    format!("{db_scheme}://{user}:{password}@{host}:{port}/{db}")
                }
            }
            Driver::Redis | Driver::Valkey | Driver::Dragonfly => {
                let user = encode_url_component(self.username.as_deref().unwrap_or(""));
                let password = encode_url_component(self.password.as_deref().unwrap_or(""));
                if user.is_empty() && password.is_empty() {
                    format!("redis://{host}:{port}")
                } else if user.is_empty() {
                    format!("redis://:{password}@{host}:{port}")
                } else {
                    format!("redis://{user}:{password}@{host}:{port}")
                }
            }
            Driver::Memcached => format!("memcached://{host}:{port}"),
            Driver::Minio | Driver::Garage | Driver::Rustfs | Driver::Localstack => {
                format!("{scheme}://{host}:{port}")
            }
            Driver::Opensearch => format!("{scheme}://{host}:{port}"),
            Driver::Elasticsearch => format!("{scheme}://{host}:{port}"),
            Driver::Meilisearch => format!("{scheme}://{host}:{port}"),
            Driver::Typesense => format!("{scheme}://{host}:{port}"),
            Driver::Rabbitmq => {
                let user = encode_url_component(self.username.as_deref().unwrap_or("guest"));
                let password = encode_url_component(self.password.as_deref().unwrap_or("guest"));
                format!("amqp://{user}:{password}@{host}:{port}")
            }
            Driver::Soketi => format!("{scheme}://{host}:{port}"),
            Driver::Frankenphp
            | Driver::Reverb
            | Driver::Horizon
            | Driver::Scheduler
            | Driver::Dusk
            | Driver::Gotenberg
            | Driver::Mailhog => {
                format!("{scheme}://{host}:{port}")
            }
        }
    }
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        return format!("[{host}]");
    }

    host.to_owned()
}

fn encode_url_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    value.bytes().fold(String::new(), |mut acc, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            acc.push(char::from(byte));
            return acc;
        }

        acc.push('%');
        acc.push(char::from(HEX[usize::from(byte >> 4)]));
        acc.push(char::from(HEX[usize::from(byte & 0x0F)]));
        acc
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn connection_url_encodes_credentials_and_brackets_ipv6_hosts() -> anyhow::Result<()> {
        let mut service = crate::config::preset_preview("mysql")?;
        service.host = "::1".to_owned();
        service.port = 3306;
        service.username = Some("laravel@team".to_owned());
        service.password = Some("p@ss/word".to_owned());
        service.database = Some("my/db".to_owned());

        assert_eq!(
            service.connection_url(),
            "mysql://laravel%40team:p%40ss%2Fword@[::1]:3306/my%2Fdb"
        );
        Ok(())
    }

    #[test]
    fn connection_url_uses_sqlsrv_scheme_for_sqlserver() -> anyhow::Result<()> {
        let mut service = crate::config::preset_preview("sqlserver")?;
        service.host = "127.0.0.1".to_owned();
        service.port = 1433;
        service.database = Some("app".to_owned());

        assert_eq!(
            service.connection_url(),
            "sqlsrv://sa:BerthSqlServerPassw0rd%21@127.0.0.1:1433/app"
        );
        Ok(())
    }
}
