//! HTTP readiness probe for managed host listeners.
//!
//! The probe is one bounded request to an owned loopback socket. It proves
//! only that the socket answered with the declared status line. Ownership,
//! route publication and identity checks stay with the supervisor.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use effigy_containers::EffectiveManagedHostListener;
use serde::{Deserialize, Serialize};

const CONNECT_TIMEOUT: Duration = Duration::from_millis(300);
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(500);
const STATUS_LINE_LIMIT: usize = 512;

/// Probe phase outcome. Tokens are stable and appear in listener diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum HttpReadinessOutcome {
    Ready,
    ConnectFailed,
    WriteFailed,
    NoStatusLine,
    MalformedStatus,
    StatusMismatch,
}

impl HttpReadinessOutcome {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::ConnectFailed => "connect_failed",
            Self::WriteFailed => "write_failed",
            Self::NoStatusLine => "no_status_line",
            Self::MalformedStatus => "malformed_status",
            Self::StatusMismatch => "status_mismatch",
        }
    }
}

/// One probe attempt. `status` is set only when a well-formed status line
/// arrived; no response body or header is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HttpReadinessProbe {
    pub(super) outcome: HttpReadinessOutcome,
    pub(super) status: Option<u16>,
}

pub(super) fn probe_http_readiness(
    address: SocketAddr,
    domain: &str,
    config: &EffectiveManagedHostListener,
) -> HttpReadinessProbe {
    let probe = |outcome| HttpReadinessProbe {
        outcome,
        status: None,
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) else {
        return probe(HttpReadinessOutcome::ConnectFailed);
    };
    let deadline = Instant::now() + RESPONSE_TIMEOUT;
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        config.readiness_path, domain
    );
    if stream.set_write_timeout(Some(RESPONSE_TIMEOUT)).is_err()
        || stream.write_all(request.as_bytes()).is_err()
    {
        return probe(HttpReadinessOutcome::WriteFailed);
    }
    // The request side stays open. `Connection: close` already ends the
    // request, and ordinary HTTP servers may drop a pending response once they
    // observe EOF on the request side.
    let Some(line) = read_status_line(&mut stream, deadline) else {
        return probe(HttpReadinessOutcome::NoStatusLine);
    };
    match parse_status_code(&line) {
        None => probe(HttpReadinessOutcome::MalformedStatus),
        Some(status) if status == config.readiness_status => HttpReadinessProbe {
            outcome: HttpReadinessOutcome::Ready,
            status: Some(status),
        },
        Some(status) => HttpReadinessProbe {
            outcome: HttpReadinessOutcome::StatusMismatch,
            status: Some(status),
        },
    }
}

/// Read until the first line break, the response deadline, or the status-line
/// limit. A response may arrive in any number of writes.
fn read_status_line(stream: &mut TcpStream, deadline: Instant) -> Option<Vec<u8>> {
    let mut line = Vec::with_capacity(64);
    let mut chunk = [0u8; 128];
    while !line.contains(&b'\n') {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        stream.set_read_timeout(Some(remaining)).ok()?;
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return None,
            Ok(count) => {
                line.extend_from_slice(&chunk[..count]);
                if line.len() > STATUS_LINE_LIMIT {
                    return None;
                }
            }
        }
    }
    let end = line.iter().position(|byte| *byte == b'\n')?;
    line.truncate(end + 1);
    Some(line)
}

fn parse_status_code(line: &[u8]) -> Option<u16> {
    let text = std::str::from_utf8(line)
        .ok()?
        .trim_end_matches(['\r', '\n']);
    let mut parts = text.split(' ');
    let version = parts.next()?.as_bytes();
    if version.len() != 8 || !version.starts_with(b"HTTP/1.") || !version[7].is_ascii_digit() {
        return None;
    }
    let status = parts.next()?;
    if status.len() != 3 || !status.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    status.parse::<u16>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::net::{Shutdown, TcpListener};
    use std::thread;

    use bytes::Bytes;
    use http_body_util::Full;
    use hyper::body::Incoming;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    const DOMAIN: &str = "managed-readiness.test";

    fn listener_config(status: u16) -> EffectiveManagedHostListener {
        EffectiveManagedHostListener {
            bind: "127.0.0.1:0".to_owned(),
            readiness_path: "/health".to_owned(),
            readiness_status: status,
            readiness_timeout_secs: 5,
            route_domain: DOMAIN.to_owned(),
            route_tls: false,
        }
    }

    /// Ordinary HTTP/1 server with hyper's default connection behavior, the
    /// same engine Axum's default `serve` uses for HTTP/1 requests. It runs on
    /// its own thread and is leaked for the life of the test process.
    fn spawn_hyper_http1_server(status: u16) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind hyper fixture");
        listener
            .set_nonblocking(true)
            .expect("nonblocking hyper fixture");
        let address = listener.local_addr().expect("hyper fixture address");
        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("hyper fixture runtime");
            runtime.block_on(async move {
                let listener =
                    tokio::net::TcpListener::from_std(listener).expect("tokio fixture listener");
                loop {
                    let (stream, _) = listener.accept().await.expect("accept hyper fixture");
                    tokio::spawn(async move {
                        let service = service_fn(move |_request: Request<Incoming>| async move {
                            Ok::<_, Infallible>(
                                Response::builder()
                                    .status(status)
                                    .body(Full::new(Bytes::from_static(b"ready")))
                                    .expect("fixture response"),
                            )
                        });
                        let _ = http1::Builder::new()
                            .serve_connection(TokioIo::new(stream), service)
                            .await;
                    });
                }
            });
        });
        address
    }

    /// Raw TCP controls for responses that a conforming HTTP client must
    /// handle, or must reject, but that a full HTTP server would not produce on
    /// demand. Each control runs on its own thread and is leaked for the test
    /// process.
    fn spawn_raw_server(script: fn(TcpStream)) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind raw fixture");
        let address = listener.local_addr().expect("raw fixture address");
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                script(stream);
            }
        });
        address
    }

    fn read_request_head(stream: &mut TcpStream) {
        let mut seen = Vec::new();
        let mut byte = [0u8; 1];
        while !seen.ends_with(b"\r\n\r\n") && seen.len() < 16 * 1024 {
            match stream.read(&mut byte) {
                Ok(1) => seen.push(byte[0]),
                _ => return,
            }
        }
    }

    fn probe_outcome(address: SocketAddr, status: u16) -> HttpReadinessOutcome {
        probe_http_readiness(address, DOMAIN, &listener_config(status)).outcome
    }

    #[test]
    fn managed_host_readiness_accepts_ordinary_http1_server_response() {
        let address = spawn_hyper_http1_server(200);
        let probe = probe_http_readiness(address, DOMAIN, &listener_config(200));
        assert_eq!(probe.outcome, HttpReadinessOutcome::Ready);
        assert_eq!(probe.status, Some(200));
    }

    #[test]
    fn managed_host_readiness_rejects_wrong_status_from_ordinary_server() {
        let address = spawn_hyper_http1_server(503);
        let probe = probe_http_readiness(address, DOMAIN, &listener_config(200));
        assert_eq!(probe.outcome, HttpReadinessOutcome::StatusMismatch);
        assert_eq!(probe.status, Some(503));
    }

    /// Mechanism control for the old probe: once the request side is
    /// half-closed, the ordinary HTTP/1 server returns no response bytes even
    /// though the same server answers a complete request with 200.
    #[test]
    fn managed_host_readiness_half_closed_request_gets_no_ordinary_response() {
        let address = spawn_hyper_http1_server(200);
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(300))
            .expect("connect hyper fixture");
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .expect("read timeout");
        stream
            .write_all(
                format!("GET /health HTTP/1.1\r\nHost: {DOMAIN}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .expect("write request");
        stream
            .shutdown(Shutdown::Write)
            .expect("half-close request");
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        assert!(
            response.is_empty(),
            "half-closed request received {} response bytes",
            response.len()
        );
    }

    #[test]
    fn managed_host_readiness_accepts_status_line_split_across_writes() {
        let address = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
            let _ = stream.write_all(b"HTTP/1.1 ");
            let _ = stream.flush();
            thread::sleep(Duration::from_millis(100));
            let _ = stream.write_all(b"200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        });
        assert_eq!(probe_outcome(address, 200), HttpReadinessOutcome::Ready);
    }

    #[test]
    fn managed_host_readiness_rejects_malformed_eof_and_silent_responses() {
        let malformed = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
            let _ = stream.write_all(b"not http\r\n\r\n");
        });
        let closed = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
        });
        let silent = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
            thread::sleep(Duration::from_secs(2));
        });
        assert_eq!(
            probe_outcome(malformed, 200),
            HttpReadinessOutcome::MalformedStatus
        );
        assert_eq!(
            probe_outcome(closed, 200),
            HttpReadinessOutcome::NoStatusLine
        );
        assert_eq!(
            probe_outcome(silent, 200),
            HttpReadinessOutcome::NoStatusLine
        );
    }

    #[test]
    fn managed_host_readiness_status_line_parser_is_strict_and_bounded() {
        assert_eq!(parse_status_code(b"HTTP/1.1 200 OK\r\n"), Some(200));
        assert_eq!(parse_status_code(b"HTTP/1.0 204\r\n"), Some(204));
        assert_eq!(parse_status_code(b"HTTP/2 200\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.bad 200 OK\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1. 200 OK\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.11 200 OK\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.1 20\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.1 2000\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.1 ABC\r\n"), None);
        assert_eq!(parse_status_code(b"HTTP/1.1\r\n"), None);
        assert_eq!(parse_status_code(b"\xff\xfe 200\r\n"), None);
    }

    /// A malformed HTTP version must fail closed even though its status digits
    /// look valid; the old prefix-only grammar accepted this as 200.
    #[test]
    fn managed_host_readiness_rejects_malformed_http_version_with_valid_status() {
        let address = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
            let _ = stream.write_all(b"HTTP/1.bad 200 OK\r\nContent-Length: 0\r\n\r\n");
        });
        assert_eq!(
            probe_outcome(address, 200),
            HttpReadinessOutcome::MalformedStatus
        );
    }

    #[test]
    fn managed_host_readiness_rejects_oversized_status_line_without_retaining_it() {
        let address = spawn_raw_server(|mut stream| {
            read_request_head(&mut stream);
            let _ = stream.write_all(&vec![b'a'; 2 * STATUS_LINE_LIMIT]);
            let _ = stream.write_all(b"\r\n");
        });
        assert_eq!(
            probe_outcome(address, 200),
            HttpReadinessOutcome::NoStatusLine
        );
    }
}
