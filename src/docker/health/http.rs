//! docker health http module.
//!
//! Contains docker health http logic used by Berth command workflows.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};

pub(super) fn http_status_code(host: &str, port: u16, path: &str) -> Result<u16> {
    let mut addrs = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("failed resolving {host}:{port}"))?;
    let addr = addrs
        .next()
        .ok_or_else(|| anyhow::anyhow!("no socket addresses resolved for {host}:{port}"))?;

    let timeout = std::time::Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("failed connecting to {host}:{port}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("failed setting read timeout")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("failed setting write timeout")?;

    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .context("failed writing health check request")?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .context("failed reading health check response")?;

    let code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP status line: {status_line}"))?
        .parse::<u16>()
        .context("failed parsing HTTP status code")?;

    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::http_status_code;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    use std::{thread, thread::JoinHandle};

    fn with_http_server<F, R>(response: &str, requests: usize, test: F) -> R
    where
        F: FnOnce(u16) -> R,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let port = listener
            .local_addr()
            .expect("listener local address")
            .port();

        let response = response.to_owned();
        let handle: JoinHandle<()> = thread::spawn(move || {
            let mut remaining = requests;
            let timeout = Instant::now() + Duration::from_secs(1);
            while remaining > 0 && Instant::now() < timeout {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buffer = [0u8; 512];
                        let _ = stream.read(&mut buffer).unwrap_or(0);
                        let _ = stream.write_all(response.as_bytes()).is_ok();
                        let _ = stream.flush().is_ok();
                        remaining -= 1;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        let result = test(port);
        handle.join().expect("server thread");
        result
    }

    #[test]
    fn parses_http_status_code_from_response() {
        with_http_server("HTTP/1.1 204 No Content\r\n\r\n", 1, |port| {
            let status =
                http_status_code("127.0.0.1", port, "/").expect("status code from server response");

            assert_eq!(status, 204);
        });
    }

    #[test]
    fn rejects_invalid_status_line() {
        with_http_server("INVALID\r\n", 1, |port| {
            assert!(http_status_code("127.0.0.1", port, "/").is_err());
        });
    }

    #[test]
    fn rejects_unresolvable_host() {
        let result = http_status_code("256.0.0.1", 80, "/");
        assert!(result.is_err());
    }
}
