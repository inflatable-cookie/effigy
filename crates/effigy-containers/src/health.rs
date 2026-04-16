//! Container health check probing.
//!
//! Extracted from `src/runner/container_command.rs` — these are
//! container-domain health check operations, not CLI shell behavior.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Probe a health check once and return whether the service is healthy.
pub fn probe_health(check: &str) -> bool {
    if let Some((host, port, path)) = parse_http_health_check(check) {
        return probe_http(&host, port, &path);
    }
    if let Some((host, port)) = parse_tcp_health_check(check) {
        return probe_tcp(&host, port);
    }
    false
}

/// Quick health status check — returns a label for display.
pub fn probe_health_status(check: Option<&str>) -> Option<&'static str> {
    match check {
        None => None,
        Some(check) if probe_health(check) => Some("ready"),
        Some(_) => Some("waiting"),
    }
}

/// Wait for a container health check to pass within a timeout.
///
/// Returns `Some("ready")` on success, `Some("interrupted")` if the
/// stop flag is set, or an error if the timeout expires.
pub fn wait_for_ready(
    container_name: &str,
    check: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, String> {
    let Some(check) = check else {
        return Ok(None);
    };
    let started = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    while started.elapsed() <= timeout {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            return Ok(Some("interrupted"));
        }
        if probe_health(check) {
            return Ok(Some("ready"));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!(
        "container `{container_name}` health check `{check}` did not become ready within {timeout_secs}s"
    ))
}

/// Parse an HTTP health check URL.
///
/// Returns `(host, port, path)` or None if not an HTTP URL.
pub fn parse_http_health_check(check: &str) -> Option<(String, u16, String)> {
    let rest = check.strip_prefix("http://")?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = split_host_port(authority, 80)?;
    Some((host, port, path))
}

/// Parse a TCP health check address.
///
/// Returns `(host, port)` or None if not a TCP check.
pub fn parse_tcp_health_check(check: &str) -> Option<(String, u16)> {
    let rest = check.strip_prefix("tcp://")?;
    split_host_port(rest, 0)
}

/// Split an authority string into host and port.
///
/// Uses `default_port` if no port is specified. Returns None if the
/// port is required but missing (when `default_port` is 0).
pub fn split_host_port(authority: &str, default_port: u16) -> Option<(String, u16)> {
    if let Some((host, port)) = authority.rsplit_once(':') {
        let port = port.parse::<u16>().ok()?;
        return Some((host.to_owned(), port));
    }
    if default_port == 0 {
        return None;
    }
    Some((authority.to_owned(), default_port))
}

fn probe_http(host: &str, port: u16, path: &str) -> bool {
    let mut stream = match connect_tcp(host, port) {
        Some(stream) => stream,
        None => return false,
    };
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 256];
    let read = stream.read(&mut buf).ok().unwrap_or(0);
    if read == 0 {
        return false;
    }
    let head = String::from_utf8_lossy(&buf[..read]);
    head.starts_with("HTTP/1.1 2")
        || head.starts_with("HTTP/1.0 2")
        || head.starts_with("HTTP/1.1 3")
        || head.starts_with("HTTP/1.0 3")
}

fn probe_tcp(host: &str, port: u16) -> bool {
    connect_tcp(host, port).is_some()
}

fn connect_tcp(host: &str, port: u16) -> Option<TcpStream> {
    let addr = format!("{host}:{port}");
    let addrs = addr.to_socket_addrs().ok()?;
    for candidate in addrs {
        if let Ok(stream) = TcpStream::connect_timeout(&candidate, Duration::from_millis(400)) {
            let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
            let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
            return Some(stream);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_health_check_with_path() {
        assert_eq!(
            parse_http_health_check("http://localhost:8080/health"),
            Some(("localhost".to_owned(), 8080, "/health".to_owned()))
        );
    }

    #[test]
    fn parse_http_health_check_pathless() {
        assert_eq!(
            parse_http_health_check("http://localhost:8080"),
            Some(("localhost".to_owned(), 8080, "/".to_owned()))
        );
    }

    #[test]
    fn parse_http_health_check_default_port() {
        assert_eq!(
            parse_http_health_check("http://localhost"),
            Some(("localhost".to_owned(), 80, "/".to_owned()))
        );
    }

    #[test]
    fn parse_http_health_check_not_http() {
        assert_eq!(parse_http_health_check("tcp://localhost:3306"), None);
    }

    #[test]
    fn parse_tcp_health_check_with_port() {
        assert_eq!(
            parse_tcp_health_check("tcp://127.0.0.1:5432"),
            Some(("127.0.0.1".to_owned(), 5432))
        );
    }

    #[test]
    fn parse_tcp_health_check_no_port_returns_none() {
        assert_eq!(parse_tcp_health_check("tcp://localhost"), None);
    }

    #[test]
    fn parse_tcp_health_check_not_tcp() {
        assert_eq!(parse_tcp_health_check("http://localhost:80"), None);
    }

    #[test]
    fn split_host_port_with_explicit_port() {
        assert_eq!(
            split_host_port("localhost:3000", 80),
            Some(("localhost".to_owned(), 3000))
        );
    }

    #[test]
    fn split_host_port_uses_default() {
        assert_eq!(
            split_host_port("localhost", 80),
            Some(("localhost".to_owned(), 80))
        );
    }

    #[test]
    fn split_host_port_requires_port_when_default_zero() {
        assert_eq!(split_host_port("localhost", 0), None);
    }

    #[test]
    fn probe_health_status_none_returns_none() {
        assert_eq!(probe_health_status(None), None);
    }
}
