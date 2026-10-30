//! Container health check probing.
//!
//! Extracted from `src/runner/container_command.rs` — these are
//! container-domain health check operations, not CLI shell behavior.

pub mod readiness;

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// The answer one probe of a declared health check gave.
///
/// A definite status line is evidence, not an unfinished attempt: the service
/// answered the request Effigy sent, so waiting cannot change that answer
/// (effigy#455). A probe with no answer at all keeps its existing budgeted
/// wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthProbe {
    /// The check answered in the ready class its form declares.
    Ready,
    /// The check answered a definite status line outside the ready class,
    /// reported with the status it observed.
    StatusMismatch(u16),
    /// The check did not answer: a closed port, an incomplete response, or a
    /// form this probe cannot test.
    Unanswered,
}

/// Probe a health check once and return whether the service is healthy.
pub fn probe_health(check: &str) -> bool {
    probe_health_detail(check) == HealthProbe::Ready
}

/// Probe a health check once, keeping the answer the service gave.
pub fn probe_health_detail(check: &str) -> HealthProbe {
    if let Some((host, port, path)) = parse_http_health_check(check) {
        return probe_http(&host, port, &path);
    }
    if let Some((host, port)) = parse_tcp_health_check(check) {
        return if probe_tcp(&host, port) {
            HealthProbe::Ready
        } else {
            HealthProbe::Unanswered
        };
    }
    HealthProbe::Unanswered
}

/// Quick health status check — returns a label for display.
pub fn probe_health_status(check: Option<&str>) -> Option<&'static str> {
    match check {
        None => None,
        Some(check) if probe_health(check) => Some("ready"),
        Some(_) => Some("waiting"),
    }
}

/// Why a declared container health check did not come ready.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthWaitError {
    /// The declared budget was spent without the check answering ready.
    NotReady {
        subject: String,
        check: String,
        timeout_secs: u64,
    },
    /// The check answered a definite status line outside its ready class, so
    /// the declared budget is not spent (effigy#455).
    StatusMismatch {
        subject: String,
        check: String,
        status: u16,
        timeout_secs: u64,
    },
}

impl HealthWaitError {
    /// What the check belonged to: a container identity, or the honest
    /// environment-and-service description the caller supplied when the
    /// runtime could not name the container.
    pub fn subject(&self) -> &str {
        match self {
            Self::NotReady { subject, .. } | Self::StatusMismatch { subject, .. } => subject,
        }
    }

    /// The declared check itself.
    pub fn check(&self) -> &str {
        match self {
            Self::NotReady { check, .. } | Self::StatusMismatch { check, .. } => check,
        }
    }

    /// How the wait ended, as a stable token.
    pub fn outcome(&self) -> &'static str {
        match self {
            Self::NotReady { .. } => "not_ready",
            Self::StatusMismatch { .. } => "status_mismatch",
        }
    }

    /// The status the check answered, when it answered a definite one.
    pub fn observed_status(&self) -> Option<u16> {
        match self {
            Self::NotReady { .. } => None,
            Self::StatusMismatch { status, .. } => Some(*status),
        }
    }

    /// The declared budget this wait had.
    pub fn timeout_secs(&self) -> u64 {
        match self {
            Self::NotReady { timeout_secs, .. } | Self::StatusMismatch { timeout_secs, .. } => {
                *timeout_secs
            }
        }
    }
}

impl std::fmt::Display for HealthWaitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotReady {
                subject,
                check,
                timeout_secs,
            } => write!(
                formatter,
                "the health check of {subject} (`{check}`) did not become ready within {timeout_secs}s"
            ),
            Self::StatusMismatch {
                subject,
                check,
                status,
                timeout_secs,
            } => write!(
                formatter,
                "the health check of {subject} (`{check}`) answered HTTP {status} where a 2xx or 3xx status is required (status_mismatch); a definite answer is terminal, so the declared {timeout_secs}s budget is not spent"
            ),
        }
    }
}

/// Wait for a container health check to pass within a timeout.
///
/// `subject` is what the check belongs to: the container identity the runtime
/// reports, or an honest environment-and-service description when the runtime
/// could not name one. Frames and the refusal use that phrase instead of
/// calling an environment a container (effigy#460).
///
/// Returns `Some("ready")` on success, `Some("interrupted")` if the
/// stop flag is set, or the reason the check did not come ready. A check that
/// answers a definite status line outside its ready class ends the wait at
/// once and reports what it answered (effigy#455).
pub fn wait_for_ready(
    subject: &str,
    check: Option<&str>,
    timeout_secs: u64,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
) -> Result<Option<&'static str>, HealthWaitError> {
    let Some(check) = check else {
        return Ok(None);
    };
    let started = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);
    // A health wait is exactly the work the operator needs to watch: the frame
    // names the probe now in flight and its attempt, and keeps the last answer
    // the check gave, with elapsed time (effigy#460, Q-017).
    let progress = effigy_ui::steps::ProgressSource::waiting_for(format!(
        "`{check}` to report {subject} healthy"
    ));
    let step = effigy_ui::steps::live_step(progress.clone());
    let mut attempts = 0_u32;
    while started.elapsed() <= timeout {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            step.finish_clear();
            return Ok(Some("interrupted"));
        }
        attempts += 1;
        progress.update(
            format!("probing `{check}` for {subject} (attempt {attempts})"),
            None,
            None,
        );
        match probe_health_detail(check) {
            HealthProbe::Ready => {
                step.succeed(&format!("{subject} reports healthy"));
                return Ok(Some("ready"));
            }
            // The service bound and answered this request with another
            // status: spending the budget cannot change that answer, so the
            // wait ends with the status it observed (effigy#455).
            HealthProbe::StatusMismatch(status) => {
                let error = HealthWaitError::StatusMismatch {
                    subject: subject.to_owned(),
                    check: check.to_owned(),
                    status,
                    timeout_secs,
                };
                step.fail(&format!(
                    "{subject} answered HTTP {status} for `{check}`, not a ready status"
                ));
                return Err(error);
            }
            HealthProbe::Unanswered => {
                progress.event(format!("`{check}` did not answer healthy"));
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    step.fail(&format!(
        "{subject} did not report healthy within {timeout_secs}s ({check})"
    ));
    Err(HealthWaitError::NotReady {
        subject: subject.to_owned(),
        check: check.to_owned(),
        timeout_secs,
    })
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

fn probe_http(host: &str, port: u16, path: &str) -> HealthProbe {
    let Some(mut stream) = connect_tcp(host, port) else {
        return HealthProbe::Unanswered;
    };
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return HealthProbe::Unanswered;
    }
    let mut buf = [0u8; 256];
    let read = stream.read(&mut buf).ok().unwrap_or(0);
    if read == 0 {
        return HealthProbe::Unanswered;
    }
    let head = String::from_utf8_lossy(&buf[..read]);
    match http_status(&head) {
        Some(status) if (200..400).contains(&status) => HealthProbe::Ready,
        Some(status) => HealthProbe::StatusMismatch(status),
        None => HealthProbe::Unanswered,
    }
}

/// The status code of a complete HTTP/1.x status line, or `None` when no
/// definite status line arrived.
fn http_status(head: &str) -> Option<u16> {
    let line = head.lines().next()?;
    let mut parts = line.split(' ');
    let version = parts.next()?;
    if version != "HTTP/1.0" && version != "HTTP/1.1" {
        return None;
    }
    let status = parts.next()?;
    if status.len() != 3 || !status.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    status.parse().ok()
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
#[path = "health/tests.rs"]
mod tests;
