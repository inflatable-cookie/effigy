//! Anonymous, memory-only parent capabilities. The inherited descriptor and
//! its live native peer are authority; environment text only selects a grant.
use crate::ctx::{Capability, EffectContext, OperationContext};
use crate::id::Digest;
use crate::ident::{self, ProcessIdentity};
use crate::observe::Observed;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

pub const ENV: &str = "EFFIGY_PARENT_CAPABILITY";
pub(crate) const CHILD_FD: i32 = 9;
const PREFIX: &str = "memory-v1:9:";
const MAX: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Grant {
    version: u8,
    owner: ProcessIdentity,
    payload: serde_json::Value,
}

/// Only the owner retains the writing endpoint. Dropping it revokes every
/// reader, including readers which still have the grant queued in memory.
#[derive(Debug)]
pub struct MemoryLease {
    _owner: UnixStream,
    reader: InheritedCapability,
}

/// A clone retains the reading endpoint only, never the owner's liveness.
/// Payload bytes are peeked, so siblings and descendants cannot consume each
/// other's grant during normal validation. No pathname or journal is involved.
#[derive(Clone)]
pub struct InheritedCapability {
    stream: Arc<UnixStream>,
    marker: String,
}
impl std::fmt::Debug for InheritedCapability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InheritedCapability { payload: [REDACTED] }")
    }
}

impl MemoryLease {
    pub fn new(cx: &EffectContext, payload: &impl Serialize) -> io::Result<Self> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| refused("parent capability requires host authority"))?;
        cx.operation()
            .check("parent capability")
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let owner = match ident::process_identity(std::process::id()) {
            Observed::Verified(owner, _) => owner,
            _ => return Err(refused("parent identity unavailable")),
        };
        let bytes = serde_json::to_vec(&Grant {
            version: 1,
            owner,
            payload: serde_json::to_value(payload)?,
        })?;
        if bytes.len() > MAX {
            return Err(refused("parent capability exceeds limit"));
        }
        let marker = marker(&bytes);
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(&bytes);
        let (mut owner, reader) = UnixStream::pair()?;
        owner.set_nonblocking(true)?;
        reader.set_nonblocking(true)?;
        // The frame is bounded below the native socket send buffer. A short
        // or failed write refuses construction; no partially sent grant escapes.
        owner.write_all(&frame)?;
        Ok(Self {
            _owner: owner,
            reader: InheritedCapability {
                stream: Arc::new(reader),
                marker,
            },
        })
    }
    pub fn capability(&self) -> InheritedCapability {
        self.reader.clone()
    }
}

impl InheritedCapability {
    /// Capture this executable's reserved inherited descriptor. A forged
    /// marker, wrong descriptor type, dead peer or mismatched grant refuses.
    /// Successful capture marks the original descriptor CLOEXEC; further
    /// inheritance happens only through Custody's explicit spawn capability.
    pub fn capture(cx: &OperationContext, expected: &str) -> io::Result<Self> {
        if expected.len() != PREFIX.len() + 64
            || !expected.starts_with(PREFIX)
            || !expected[PREFIX.len()..]
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(refused("invalid parent capability selector"));
        }
        // SAFETY: fcntl validates the numeric inherited slot without borrowing
        // it. Success returns a fresh CLOEXEC descriptor owned exactly once.
        let fd = unsafe { libc::fcntl(CHILD_FD, libc::F_DUPFD_CLOEXEC, 128) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful fcntl transferred this fresh descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let reader = Self::from_descriptor(cx, fd, expected)?;
        // SAFETY: the protocol's original slot is still open in this process.
        // CLOEXEC stops ambient inheritance; the captured copy is independent.
        if unsafe { libc::fcntl(CHILD_FD, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(reader)
    }
    fn from_descriptor(cx: &OperationContext, fd: OwnedFd, expected: &str) -> io::Result<Self> {
        let reader = Self {
            stream: Arc::new(UnixStream::from(fd)),
            marker: expected.to_owned(),
        };
        reader.grant(cx)?;
        Ok(reader)
    }
    pub fn marker(&self) -> &str {
        &self.marker
    }
    pub fn payload<T: DeserializeOwned>(&self, cx: &OperationContext) -> io::Result<T> {
        serde_json::from_value(self.grant(cx)?.payload).map_err(io::Error::other)
    }
    pub(crate) fn verify(&self, cx: &OperationContext) -> io::Result<()> {
        self.grant(cx).map(|_| ())
    }
    pub(crate) fn custody_fd(&self, cx: &OperationContext) -> io::Result<OwnedFd> {
        self.verify(cx)?;
        crate::gate::duplicate_custody(self.stream.as_ref())
    }
    fn grant(&self, cx: &OperationContext) -> io::Result<Grant> {
        cx.check("inherited parent capability")
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        let fd = self.stream.as_raw_fd();
        if socket_option(fd, libc::SOL_SOCKET, libc::SO_TYPE)? != libc::SOCK_STREAM {
            return Err(refused("parent capability is not a stream socket"));
        }
        live_socket(fd)?;
        let mut bytes = [0; MAX + 4];
        // SAFETY: live owned socket, exclusively writable initialized buffer
        // of this exact length. PEEK never consumes another child's grant;
        // DONTWAIT bounds this read even for a substituted blocking descriptor.
        let count = unsafe {
            libc::recv(
                fd,
                bytes.as_mut_ptr().cast(),
                bytes.len(),
                libc::MSG_PEEK | libc::MSG_DONTWAIT,
            )
        };
        if count < 4 {
            return Err(refused("parent capability frame unavailable"));
        }
        let length = u32::from_be_bytes(bytes[..4].try_into().expect("four bytes")) as usize;
        if length > MAX || count as usize != length + 4 {
            return Err(refused("parent capability frame incomplete"));
        }
        let bytes = &bytes[4..length + 4];
        if marker(bytes) != self.marker {
            return Err(refused("parent capability selector disagrees"));
        }
        let grant: Grant = serde_json::from_slice(bytes)?;
        if grant.version != 1 || peer_pid(fd)? != grant.owner.pid {
            return Err(refused("parent capability peer disagrees"));
        }
        match ident::process_identity(grant.owner.pid) {
            Observed::Verified(actual, _) if actual == grant.owner => {}
            _ => return Err(refused("parent capability owner is unavailable")),
        }
        live_socket(fd)?;
        cx.check("inherited parent capability")
            .map_err(|error| io::Error::other(format!("{error:?}")))?;
        Ok(grant)
    }
}

fn marker(bytes: &[u8]) -> String {
    let mut marker = PREFIX.to_owned();
    for byte in Digest::of(bytes).0 {
        marker.push_str(&format!("{byte:02x}"));
    }
    marker
}
fn refused(reason: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, reason)
}
fn live_socket(fd: i32) -> io::Result<()> {
    let mut poll = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: one initialized writable pollfd, with a nonblocking timeout.
    if unsafe { libc::poll(&mut poll, 1, 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if poll.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
        return Err(refused("parent capability was revoked"));
    }
    Ok(())
}
fn socket_option(fd: i32, level: i32, option: i32) -> io::Result<i32> {
    let mut value = 0i32;
    let mut size = std::mem::size_of_val(&value) as libc::socklen_t;
    // SAFETY: getsockopt validates fd, and receives an aligned initialized i32
    // buffer whose exact writable extent is given by size. Check returned size.
    if unsafe {
        libc::getsockopt(
            fd,
            level,
            option,
            (&mut value as *mut i32).cast(),
            &mut size,
        )
    } != 0
    {
        return Err(io::Error::last_os_error());
    }
    if size as usize != std::mem::size_of_val(&value) {
        return Err(refused("invalid socket option size"));
    }
    Ok(value)
}
fn peer_pid(fd: i32) -> io::Result<u32> {
    #[cfg(target_os = "macos")]
    {
        let mut uid = 0;
        let mut gid = 0;
        // SAFETY: getpeereid validates the socket and writes two uid/gid values
        // into aligned, initialized, exclusively borrowed outputs.
        if unsafe { libc::getpeereid(fd, &mut uid, &mut gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if uid != crate::fs::operator_uid() {
            return Err(refused("foreign parent capability peer"));
        }
        u32::try_from(socket_option(fd, libc::SOL_LOCAL, libc::LOCAL_PEERPID)?)
            .map_err(|_| refused("invalid parent capability peer pid"))
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: zero is valid for ucred's integer fields.
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of_val(&credentials) as libc::socklen_t;
        // SAFETY: live socket and an aligned, initialized ucred buffer with
        // precisely its writable length. Check length and uid before use.
        if unsafe {
            libc::getsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut size,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        if size as usize != std::mem::size_of_val(&credentials)
            || credentials.uid != crate::fs::operator_uid()
        {
            return Err(refused("foreign parent capability peer"));
        }
        u32::try_from(credentials.pid).map_err(|_| refused("invalid parent capability peer pid"))
    }
}

#[cfg(test)]
mod tests;
