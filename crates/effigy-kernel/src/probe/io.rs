//! Read transports accept already-resolved targets; no DNS resolution, source
//! acquisition, redirects, repair requests, or ambient proxy configuration.
use super::{check, evidence, failure};
use crate::ctx::ObserveContext;
use crate::observe::{Observed, UnknownReason};
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
pub enum HttpReadMethod {
    Get,
    Head,
}
#[derive(Debug, Clone)]
pub struct StatusSet(Vec<std::ops::RangeInclusive<u16>>);
impl Default for StatusSet {
    fn default() -> Self {
        Self(vec![200..=299])
    }
}
impl StatusSet {
    pub fn new(ranges: Vec<std::ops::RangeInclusive<u16>>) -> Result<Self, UnknownReason> {
        if ranges.is_empty()
            || ranges.len() > 500
            || ranges
                .iter()
                .any(|r| r.is_empty() || *r.start() < 100 || *r.end() > 599)
        {
            return Err(UnknownReason::Unparsable("HTTP status set".into()));
        }
        Ok(Self(ranges))
    }
    pub fn contains(&self, status: u16) -> bool {
        self.0.iter().any(|r| r.contains(&status))
    }
}
/// Adapters supply targets derived from the resolved invocation, store or
/// endpoint claims. Addresses are resolved outside observation; connecting
/// never invokes a potentially unbounded system resolver. HTTP is plain HTTP
/// over the supplied address; `host` is its validated authority, not a lookup.
#[derive(Debug, Clone)]
pub enum ObservationTarget {
    Tcp(SocketAddr),
    Http {
        address: SocketAddr,
        host: String,
        path: String,
        method: HttpReadMethod,
        expect: StatusSet,
        connect_timeout: Duration,
        total_timeout: Duration,
    },
    UnixSocket(PathBuf),
    UnixSocketAt {
        directory: crate::fs::DirFd,
        name: crate::fs::FileName,
    },
    Dns {
        name: String,
        server: SocketAddr,
    },
}
#[derive(Debug, PartialEq, Eq)]
pub enum ProbeIo {
    TcpOpen,
    UnixOpen,
    Http {
        status: u16,
        ready: bool,
    },
    /// A complete, matching, non-truncated DNS wire response. Parsing records
    /// and relating them to an endpoint generation remains the adapter's job.
    Dns {
        response: Vec<u8>,
    },
}
impl ObserveContext {
    pub fn connect(&self, target: ObservationTarget) -> Observed<ProbeIo> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        let result = (|| {
            crate::fault_point!(ProbeObservation).map_err(failure)?;
            crate::fault_point!(ProbeConnect).map_err(failure)?;
            check(self, "probe connect")?;
            if crate::store::in_transaction() {
                return Err(UnknownReason::Inconsistent(
                    "network probe inside transaction".into(),
                ));
            }
            match target {
                ObservationTarget::Tcp(address) => {
                    tcp(address, self, self.remaining())?;
                    Ok(ProbeIo::TcpOpen)
                }
                ObservationTarget::Http {
                    address,
                    host,
                    path,
                    method,
                    expect,
                    connect_timeout,
                    total_timeout,
                } => {
                    let cx = self
                        .operation
                        .child(total_timeout)
                        .observe()
                        .map_err(|_| UnknownReason::Inconsistent("HTTP budget".into()))?;
                    http(address, &host, &path, method, &expect, connect_timeout, &cx)
                }
                ObservationTarget::UnixSocket(path) => {
                    unix(&path, self, false)?;
                    Ok(ProbeIo::UnixOpen)
                }
                ObservationTarget::UnixSocketAt { directory, name } => {
                    crate::socket_at::in_directory(&directory, &name, |name| {
                        unix(name, self, true)
                    })
                    .map_err(failure)??;
                    Ok(ProbeIo::UnixOpen)
                }
                ObservationTarget::Dns { name, server } => dns(&name, server, self),
            }
        })();
        match result {
            Ok(value) => Observed::Verified(value, evidence()),
            Err(reason) => Observed::Unknown(reason),
        }
    }
}
fn local(address: SocketAddr) -> Result<(), UnknownReason> {
    // Explicit loopback transport keeps Observe from becoming arbitrary source
    // acquisition. Engines route guest/control connections through local hosts.
    if !address.ip().is_loopback() || address.port() == 0 {
        return Err(UnknownReason::Untrusted(
            "observation target must be local".into(),
        ));
    }
    Ok(())
}
fn tcp(
    address: SocketAddr,
    cx: &ObserveContext,
    timeout: Duration,
) -> Result<TcpStream, UnknownReason> {
    local(address)?;
    let phase = cx
        .operation
        .child(timeout)
        .observe()
        .map_err(|_| UnknownReason::Inconsistent("connect budget".into()))?;
    loop {
        check(&phase, "TCP connect")?;
        match TcpStream::connect_timeout(&address, phase.remaining().min(Duration::from_millis(20)))
        {
            Ok(stream) => {
                stream.set_nonblocking(true).map_err(failure)?;
                return Ok(stream);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
                ) => {}
            Err(e) => return Err(failure(e)),
        }
    }
}
fn http(
    address: SocketAddr,
    host: &str,
    path: &str,
    method: HttpReadMethod,
    expect: &StatusSet,
    connect_timeout: Duration,
    cx: &ObserveContext,
) -> Result<ProbeIo, UnknownReason> {
    if host.is_empty()
        || host.len() > 255
        || !host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
        || !path.starts_with('/')
        || path.len() > 4096
        || path.bytes().any(|b| !(0x21..=0x7e).contains(&b))
        || path.contains('#')
    {
        return Err(UnknownReason::Unparsable("HTTP target".into()));
    }
    let mut stream = tcp(address, cx, connect_timeout)?;
    let method = match method {
        HttpReadMethod::Get => "GET",
        HttpReadMethod::Head => "HEAD",
    };
    let request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    let mut offset = 0;
    while offset < request.len() {
        check(cx, "HTTP request")?;
        match stream.write(&request.as_bytes()[offset..]) {
            Ok(0) => return Err(UnknownReason::Inconsistent("HTTP write closed".into())),
            Ok(n) => offset += n,
            Err(e) if retry(&e) => pause(cx),
            Err(e) => return Err(failure(e)),
        }
    }
    let max = cx
        .limits()
        .line_max
        .min(cx.limits().capture_tail)
        .min(64 * 1024);
    let mut bytes = Vec::new();
    loop {
        check(cx, "HTTP response")?;
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let header = std::str::from_utf8(&bytes[..end])
                .map_err(|_| UnknownReason::Unparsable("HTTP header".into()))?;
            let line = header.split("\r\n").next().unwrap_or("");
            let mut words = line.split(' ');
            if !matches!(words.next(), Some("HTTP/1.0" | "HTTP/1.1")) {
                return Err(UnknownReason::Unparsable("HTTP version".into()));
            }
            let status = words
                .next()
                .filter(|s| s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|s| s.parse::<u16>().ok())
                .filter(|s| (100..=599).contains(s))
                .ok_or_else(|| UnknownReason::Unparsable("HTTP status".into()))?;
            if status < 200 {
                bytes.drain(..end + 4);
                continue;
            }
            return Ok(ProbeIo::Http {
                status,
                ready: expect.contains(status),
            });
        }
        if bytes.len() >= max {
            return Err(UnknownReason::Unparsable(
                "HTTP headers exceed capture bound".into(),
            ));
        }
        let mut chunk = [0; 1024];
        let available = chunk.len().min(max - bytes.len());
        match stream.read(&mut chunk[..available]) {
            Ok(0) => return Err(UnknownReason::Unparsable("incomplete HTTP response".into())),
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(e) if retry(&e) => pause(cx),
            Err(e) => return Err(failure(e)),
        }
    }
}
fn retry(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
    )
}
fn pause(cx: &ObserveContext) {
    std::thread::sleep(cx.remaining().min(Duration::from_millis(2)));
}
fn unix(
    path: &std::path::Path,
    cx: &ObserveContext,
    verified_directory: bool,
) -> Result<(), UnknownReason> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: zero is a valid representation of sockaddr_un; fields filled below.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if (!path.is_absolute() && !verified_directory)
        || bytes.contains(&0)
        || bytes.len() >= address.sun_path.len()
    {
        return Err(UnknownReason::Unparsable("Unix socket path".into()));
    }
    address.sun_family = libc::AF_UNIX as _;
    for (slot, byte) in address.sun_path.iter_mut().zip(bytes) {
        *slot = *byte as _;
    }
    // SAFETY: documented socket family/type/protocol; successful fd owned below.
    let raw = unsafe { libc::socket(libc::AF_UNIX, libc::SOCK_STREAM, 0) };
    if raw < 0 {
        return Err(failure(io::Error::last_os_error()));
    }
    // SAFETY: fresh descriptor returned by socket, sole ownership transferred.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    super::guard::nonblocking(fd.as_raw_fd()).map_err(failure)?;
    // SAFETY: pointer to initialized sockaddr_un and exact allocation size; fd
    // is a live nonblocking AF_UNIX stream, path is NUL terminated.
    let connected = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as _,
        )
    };
    if connected == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() != Some(libc::EINPROGRESS) {
        return Err(failure(error));
    }
    loop {
        check(cx, "Unix connect")?;
        let mut poll = libc::pollfd {
            fd: fd.as_raw_fd(),
            events: libc::POLLOUT,
            revents: 0,
        };
        // SAFETY: initialized pollfd, one element and zero timeout.
        if unsafe { libc::poll(&mut poll, 1, 0) } > 0 {
            let mut error: libc::c_int = 0;
            let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
            // SAFETY: live socket and writable initialized error/size slots.
            if unsafe {
                libc::getsockopt(
                    fd.as_raw_fd(),
                    libc::SOL_SOCKET,
                    libc::SO_ERROR,
                    (&mut error as *mut libc::c_int).cast(),
                    &mut size,
                )
            } != 0
            {
                return Err(failure(io::Error::last_os_error()));
            }
            return if error == 0 {
                Ok(())
            } else {
                Err(failure(io::Error::from_raw_os_error(error)))
            };
        }
        pause(cx);
    }
}
fn dns(name: &str, server: SocketAddr, cx: &ObserveContext) -> Result<ProbeIo, UnknownReason> {
    local(server)?;
    let name = name.strip_suffix('.').unwrap_or(name);
    if name.is_empty() || name.len() > 253 {
        return Err(UnknownReason::Unparsable("DNS name".into()));
    }
    // One socket per outstanding request; replies are connected to the exact
    // gateway address and must echo the transaction id and complete question.
    let mut query = vec![0xe5, 0x21, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
    for label in name.split('.') {
        if label.is_empty()
            || label.len() > 63
            || !label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(UnknownReason::Unparsable("DNS label".into()));
        }
        query.push(label.len() as u8);
        query.extend_from_slice(label.as_bytes());
    }
    query.extend_from_slice(&[0, 0, 1, 0, 1]);
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "127.0.0.1:0"
    } else {
        "[::1]:0"
    })
    .map_err(failure)?;
    socket.connect(server).map_err(failure)?;
    socket.set_nonblocking(true).map_err(failure)?;
    loop {
        check(cx, "DNS request")?;
        match socket.send(&query) {
            Ok(n) if n == query.len() => break,
            Ok(_) => return Err(UnknownReason::Inconsistent("short DNS request".into())),
            Err(e) if retry(&e) => pause(cx),
            Err(e) => return Err(failure(e)),
        }
    }
    let max = cx.limits().capture_tail.min(65535);
    // One extra byte detects response truncation at the configured limit.
    let mut response = vec![0; max + 1];
    loop {
        check(cx, "DNS response")?;
        match socket.recv(&mut response) {
            Ok(n) => {
                if n > max
                    || n < query.len()
                    || response[..2] != query[..2]
                    || response[2] & 0xfa != 0x80
                    || response[3] & 0x0f != 0
                    || response[4..6] != [0, 1]
                    || response[12..query.len()] != query[12..]
                {
                    return Err(UnknownReason::Unparsable("DNS response".into()));
                }
                response.truncate(n);
                return Ok(ProbeIo::Dns { response });
            }
            Err(e) if retry(&e) => pause(cx),
            Err(e) => return Err(failure(e)),
        }
    }
}
