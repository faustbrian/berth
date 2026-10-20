//! Loopback ports the gateway publishes.

use anyhow::{Context, Result};

/// Default HTTPS port. Ports 80/443 are commonly held by other local
/// development managers (for example Herd's nginx); a default that fails on
/// such hosts would break every agent's `berth up`.
pub(crate) const DEFAULT_HTTPS_PORT: u16 = 8443;
/// Default HTTP port (redirects to HTTPS).
pub(crate) const DEFAULT_HTTP_PORT: u16 = 8080;

/// Ports the gateway publishes on `127.0.0.1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GatewayPorts {
    pub(crate) https: u16,
    pub(crate) http: u16,
}

/// File in the berth home recording the installation's gateway ports.
const PORTS_FILE: &str = "ports";

impl GatewayPorts {
    /// The installation's gateway ports.
    ///
    /// The ports are an installation setting, not a per-process one: once a
    /// gateway has been created they are recorded in
    /// `~/.berth/gateway/ports`, and every later process uses them, so a
    /// command run with a different environment can never move the gateway
    /// or render routes on ports it does not publish. Before that, they come
    /// from `BERTH_GATEWAY_HTTPS_PORT`/`BERTH_GATEWAY_HTTP_PORT`, or the
    /// defaults when unset.
    ///
    /// # Errors
    ///
    /// Returns an error when a variable is invalid, or names a port that
    /// differs from the recorded one.
    pub(crate) fn current() -> Result<Self> {
        let requested = Requested::from_env()?;
        // Unit tests never touch the real berth home.
        if cfg!(test) {
            return requested.resolve();
        }
        let path = crate::home::home_path(&std::path::Path::new("gateway").join(PORTS_FILE))?;
        match read_recorded(&path)? {
            Some(recorded) => requested.check_against(recorded, &path),
            None => requested.resolve(),
        }
    }

    /// Records `ports` as the installation's gateway ports once a gateway
    /// has been created with them, unless another process recorded them
    /// first. Recording only after creation keeps a failed first start (a
    /// port held by another program) from pinning a port that never served.
    ///
    /// # Errors
    ///
    /// Returns an error when the record cannot be written, or another
    /// process recorded different ports.
    pub(crate) fn record(self) -> Result<()> {
        if cfg!(test) {
            return Ok(());
        }
        let dir = crate::home::private_dir(std::path::Path::new("gateway"))?;
        let path = dir.join(PORTS_FILE);
        if let Some(recorded) = read_recorded(&path)? {
            return self.ensure_matches(recorded, &path);
        }
        let staged = dir.join(format!(".{PORTS_FILE}.{}", std::process::id()));
        crate::config::yaml::write_file_atomically(
            &staged,
            &format!("{} {}\n", self.https, self.http),
        )?;
        // `hard_link` never replaces a record another process created first.
        let linked = std::fs::hard_link(&staged, &path);
        drop(std::fs::remove_file(&staged));
        match linked {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let recorded = read_recorded(&path)?
                    .ok_or_else(|| anyhow::anyhow!("{} vanished", path.display()))?;
                self.ensure_matches(recorded, &path)
            }
            Err(error) => {
                Err(error).with_context(|| format!("failed to record {}", path.display()))
            }
        }
    }

    fn ensure_matches(self, recorded: Self, path: &std::path::Path) -> Result<()> {
        if self == recorded {
            return Ok(());
        }
        anyhow::bail!(
            "this installation's gateway uses ports {} and {} (recorded in {}), not {} and {}; \
             rerun the command",
            recorded.https,
            recorded.http,
            path.display(),
            self.https,
            self.http
        )
    }
}

/// How to move the gateway off a taken port. The first gateway records its
/// ports and every later process uses them, so once recorded the variable
/// no longer helps (it fails against the record instead).
///
/// # Errors
///
/// Returns an error when the berth home cannot be resolved or read.
pub(crate) fn port_change_advice(variable: &str) -> Result<String> {
    let path = crate::home::home_path(&std::path::Path::new("gateway").join(PORTS_FILE))?;
    if read_recorded(&path)?.is_some() {
        return Ok(format!(
            "or move this installation's gateway: its ports are fixed in {}, so bring every \
             instance down ({} lists them), remove the gateway container ({}) and that file, \
             then set {variable}",
            path.display(),
            crate::followup::global("ls"),
            crate::followup::global("gc")
        ));
    }
    Ok(format!(
        "or set {variable} to a free port and run the command again"
    ))
}

/// Returns the `https://` origin for a host on `port`, omitting 443.
#[must_use]
pub(crate) fn https_origin(host: &str, port: u16) -> String {
    if port == 443 {
        format!("https://{host}")
    } else {
        format!("https://{host}:{port}")
    }
}

/// Ports requested through the environment; `None` when a variable is unset.
struct Requested {
    https: Option<u16>,
    http: Option<u16>,
}

impl Requested {
    fn from_env() -> Result<Self> {
        Ok(Self {
            https: port_from_env("BERTH_GATEWAY_HTTPS_PORT")?,
            http: port_from_env("BERTH_GATEWAY_HTTP_PORT")?,
        })
    }

    fn resolve(&self) -> Result<GatewayPorts> {
        let ports = GatewayPorts {
            https: self.https.unwrap_or(DEFAULT_HTTPS_PORT),
            http: self.http.unwrap_or(DEFAULT_HTTP_PORT),
        };
        if ports.https == ports.http {
            anyhow::bail!(
                "BERTH_GATEWAY_HTTPS_PORT and BERTH_GATEWAY_HTTP_PORT must differ (both {})",
                ports.https
            );
        }
        Ok(ports)
    }

    fn check_against(
        &self,
        recorded: GatewayPorts,
        path: &std::path::Path,
    ) -> Result<GatewayPorts> {
        for (name, requested, actual) in [
            ("BERTH_GATEWAY_HTTPS_PORT", self.https, recorded.https),
            ("BERTH_GATEWAY_HTTP_PORT", self.http, recorded.http),
        ] {
            if let Some(requested) = requested
                && requested != actual
            {
                anyhow::bail!(
                    "{name}={requested}, but this installation's gateway uses port {actual} \
                     (recorded in {}); unset {name}, or bring every instance down, remove the \
                     gateway container and that file to change it",
                    path.display()
                );
            }
        }
        Ok(recorded)
    }
}

fn read_recorded(path: &std::path::Path) -> Result<Option<GatewayPorts>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let mut fields = text.split_whitespace().map(str::parse::<u16>);
    match (fields.next(), fields.next(), fields.next()) {
        (Some(Ok(https)), Some(Ok(http)), None) if https != 0 && http != 0 && https != http => {
            Ok(Some(GatewayPorts { https, http }))
        }
        _ => anyhow::bail!(
            "{} must hold two different ports (\"<https> <http>\")",
            path.display()
        ),
    }
}

fn port_from_env(name: &str) -> Result<Option<u16>> {
    match std::env::var(name) {
        Ok(value) => {
            let port: u16 = value
                .trim()
                .parse()
                .with_context(|| format!("{name}={value} is not a valid port"))?;
            if port == 0 {
                anyhow::bail!("{name} must not be 0");
            }
            Ok(Some(port))
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(error) => Err(error).with_context(|| format!("{name} is not valid UTF-8")),
    }
}

#[cfg(test)]
mod tests {
    use super::{GatewayPorts, Requested, https_origin, read_recorded};

    #[test]
    fn a_recorded_port_wins_and_a_conflicting_request_fails_naming_both() {
        let recorded = GatewayPorts {
            https: 18443,
            http: 18080,
        };
        let path = std::path::Path::new("/x/ports");
        let unset = Requested {
            https: None,
            http: None,
        };
        assert_eq!(
            unset.check_against(recorded, path).expect("ports"),
            recorded
        );
        let same = Requested {
            https: Some(18443),
            http: None,
        };
        assert_eq!(same.check_against(recorded, path).expect("ports"), recorded);
        let other = Requested {
            https: Some(8443),
            http: None,
        };
        let error = other.check_against(recorded, path).expect_err("conflict");
        assert!(
            error.to_string().contains("8443") && error.to_string().contains("18443"),
            "{error}"
        );
    }

    #[test]
    fn the_record_must_hold_two_different_ports() {
        let dir =
            crate::test_support::temp_root().join(format!("berth-ports-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join("ports");
        assert!(read_recorded(&path).expect("absent").is_none());
        std::fs::write(&path, "18443 18080\n").expect("write");
        assert_eq!(
            read_recorded(&path).expect("ok"),
            Some(GatewayPorts {
                https: 18443,
                http: 18080
            })
        );
        for bad in ["18443\n", "1 1\n", "a b\n", "1 2 3\n"] {
            std::fs::write(&path, bad).expect("write");
            assert!(read_recorded(&path).is_err(), "{bad:?}");
        }
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn origins_omit_only_the_default_https_port() {
        assert_eq!(
            https_origin("a.berth.localhost", 8443),
            "https://a.berth.localhost:8443"
        );
        assert_eq!(
            https_origin("a.berth.localhost", 443),
            "https://a.berth.localhost"
        );
    }
}
