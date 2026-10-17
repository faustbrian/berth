//! cli handlers docker ops port module.
//!
//! Contains port handler used by Berth command workflows.

use anyhow::Result;

use crate::{config, docker};

use super::output_json::{collect_service_json, print_pretty_json};

pub(crate) struct HandlePortOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) services: &'a [String],
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) format: &'a str,
    pub(crate) json: bool,
    pub(crate) private_port: Option<&'a str>,
}

pub(crate) fn handle_port(config: &config::Config, options: HandlePortOptions<'_>) -> Result<()> {
    let selected = super::selected_docker_services_in_scope(
        config,
        options.service,
        options.services,
        options.kind,
        options.profile,
    )?;
    let output_json = options.json || options.format.eq_ignore_ascii_case("json");
    if output_json {
        let items = collect_service_json(selected, |service| {
            let container_name = service.container_name()?;
            let output = docker::port_output(service, options.private_port)?;
            let mut mapped = Vec::new();
            for line in output.lines() {
                if let Some(parsed) = parse_port_line(line) {
                    mapped.push(serde_json::json!({
                        "service": service.name,
                        "container": container_name,
                        "private_port": parsed.private_port,
                        "host": parsed.host,
                        "host_port": parsed.host_port,
                    }));
                }
            }
            Ok(mapped)
        })?;
        return print_pretty_json(&items);
    }

    for service in selected {
        docker::port(service, options.private_port)?;
    }
    Ok(())
}

struct ParsedPortLine {
    private_port: String,
    host: String,
    host_port: u16,
}

fn parse_port_line(line: &str) -> Option<ParsedPortLine> {
    let (private_port, rhs) = line.split_once(" -> ")?;
    let split_at = rhs.rfind(':')?;
    let host = rhs.get(..split_at)?.to_owned();
    let host_port = rhs.get(split_at + 1..)?.parse::<u16>().ok()?;

    Some(ParsedPortLine {
        private_port: private_port.to_owned(),
        host,
        host_port,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_port_line;

    #[test]
    fn parses_standard_docker_port_line() {
        let parsed = parse_port_line("80/tcp -> 0.0.0.0:8080").expect("parse");
        assert_eq!(parsed.private_port, "80/tcp");
        assert_eq!(parsed.host, "0.0.0.0");
        assert_eq!(parsed.host_port, 8080);
    }
}
