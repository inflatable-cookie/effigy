//! Privileged bind-and-pass boundary. No route state or serving code runs here.
//!
//! The root-owned launchd definition fixes the operator UID. Requests have a
//! fixed 16-byte shape, bounded I/O, and exactly two listening descriptors.
//! Custody starts the operator daemon normally; it connects after exec.
use std::io::{self, IoSlice, IoSliceMut, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::sys::socket::{recvmsg, sendmsg, ControlMessage, ControlMessageOwned, MsgFlags};

use crate::listener_handoff::{self, HandedListeners, HandedSocketSpec, HandoffIdentity};
use crate::operator::error;

pub fn label(uid: u32) -> String {
    format!("dev.effigy.gateway.bind-broker.{uid}")
}
pub fn socket_path(uid: u32) -> PathBuf {
    PathBuf::from(format!("/private/var/run/{}.sock", label(uid)))
}

// The gateway's standard endpoint and its setup-installed alias range. Keep
// this policy independent of operator-writable JSON and request parameters.
fn allowed(address: SocketAddr) -> bool {
    matches!(address, SocketAddr::V4(addr) if
        (*addr.ip() == Ipv4Addr::LOCALHOST ||
         (addr.ip().octets()[..3] == [127, 1, 0] && (1..=50).contains(&addr.ip().octets()[3])))
        && matches!(addr.port(), 80 | 443))
}
fn authorize(operator: u32, peer: u32, addresses: [SocketAddr; 2]) -> io::Result<()> {
    if operator == 0
        || peer != operator
        || !addresses.into_iter().all(allowed)
        || addresses[0].port() != 80
        || addresses[1].port() != 443
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "bind broker refused request",
        ));
    }
    Ok(())
}
fn encode(addresses: [SocketAddr; 2]) -> io::Result<[u8; 16]> {
    let mut bytes = [0; 16];
    bytes[..4].copy_from_slice(b"EGB1");
    for (address, chunk) in addresses.into_iter().zip(bytes[4..].chunks_exact_mut(6)) {
        let SocketAddr::V4(address) = address else {
            return Err(error("bind broker requires IPv4"));
        };
        chunk[..4].copy_from_slice(&address.ip().octets());
        chunk[4..].copy_from_slice(&address.port().to_be_bytes());
    }
    Ok(bytes)
}
fn decode(bytes: [u8; 16]) -> io::Result<[SocketAddr; 2]> {
    if &bytes[..4] != b"EGB1" {
        return Err(error("unknown bind broker protocol"));
    }
    let mut addresses = [SocketAddr::from(([0; 4], 0)); 2];
    for (address, chunk) in addresses.iter_mut().zip(bytes[4..].chunks_exact(6)) {
        *address = SocketAddr::from((
            [chunk[0], chunk[1], chunk[2], chunk[3]],
            u16::from_be_bytes([chunk[4], chunk[5]]),
        ));
    }
    Ok(addresses)
}
fn bounded(stream: &UnixStream) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let enabled: libc::c_int = 1;
        // SAFETY: the stream descriptor remains live and enabled is an
        // initialized, aligned integer read only during this fixed-size call.
        if unsafe {
            libc::setsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                (&enabled as *const libc::c_int).cast(),
                std::mem::size_of_val(&enabled) as libc::socklen_t,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
    }

    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))
}
fn peer_uid(stream: &UnixStream) -> io::Result<u32> {
    #[cfg(target_os = "macos")]
    {
        Ok(nix::unistd::getpeereid(stream)?.0.as_raw())
    }
    #[cfg(target_os = "linux")]
    {
        Ok(nix::sys::socket::getsockopt(stream, nix::sys::socket::sockopt::PeerCredentials)?.uid())
    }
}
#[cfg(any(target_os = "macos", test))]
fn serve_one(
    stream: &mut UnixStream,
    operator: u32,
    bind: impl Fn(SocketAddr) -> io::Result<TcpListener>,
) -> io::Result<()> {
    bounded(stream)?;
    let peer = peer_uid(stream)?;
    // Reject foreign peers before even reading their request.
    if peer != operator || operator == 0 {
        return Err(error("foreign bind broker peer"));
    }
    let mut bytes = [0; 16];
    stream.read_exact(&mut bytes)?;
    let addresses = decode(bytes)?;
    authorize(operator, peer, addresses)?;
    let listeners = [bind(addresses[0])?, bind(addresses[1])?];
    let fds = listeners.each_ref().map(AsRawFd::as_raw_fd);
    // SCM_RIGHTS duplicates the descriptors into the receiver. The broker
    // drops its copies immediately and never accepts a TCP connection.
    let sent = sendmsg::<()>(
        stream.as_raw_fd(),
        &[IoSlice::new(b"1")],
        &[ControlMessage::ScmRights(&fds)],
        MsgFlags::empty(),
        None,
    )?;
    if sent != 1 {
        return Err(error("short bind broker reply"));
    }
    Ok(())
}
fn receive(stream: &mut UnixStream, addresses: [SocketAddr; 2]) -> io::Result<HandedListeners> {
    bounded(stream)?;
    stream.write_all(&encode(addresses)?)?;
    receive_reply(stream, addresses)
}
fn receive_reply(
    stream: &mut UnixStream,
    addresses: [SocketAddr; 2],
) -> io::Result<HandedListeners> {
    let mut byte = [0];
    let mut ancillary = nix::cmsg_space!([i32; 256]);
    let mut iov = [IoSliceMut::new(&mut byte)];
    let reply = recvmsg::<()>(
        stream.as_raw_fd(),
        &mut iov,
        Some(&mut ancillary),
        MsgFlags::empty(),
    )?;
    let mut received = Vec::new();
    for control in reply.cmsgs()? {
        if let ControlMessageOwned::ScmRights(fds) = control {
            for fd in fds {
                // SAFETY: recvmsg installed new, uniquely owned descriptors
                // in this process; each is converted once and closed by RAII
                // on every refusal, before raw ownership moves to adoption.
                received.push(unsafe { OwnedFd::from_raw_fd(fd) });
            }
        }
    }
    if reply.bytes != 1
        || reply
            .flags
            .intersects(MsgFlags::MSG_CTRUNC | MsgFlags::MSG_TRUNC)
        || received.len() != 2
        || byte != *b"1"
    {
        return Err(error("invalid bind broker descriptor reply"));
    }
    let https = received
        .pop()
        .ok_or_else(|| error("missing HTTPS descriptor"))?;
    let http = received
        .pop()
        .ok_or_else(|| error("missing HTTP descriptor"))?;
    let specs = [
        HandedSocketSpec {
            name: "http",
            endpoint: addresses[0],
        },
        HandedSocketSpec {
            name: "https",
            endpoint: addresses[1],
        },
    ];
    listener_handoff::validate_and_adopt(
        &specs,
        &[
            ("http", vec![http.into_raw_fd()]),
            ("https", vec![https.into_raw_fd()]),
        ],
        &HandoffIdentity::current(),
    )
    .map_err(error)
}
/// Connect only to setup's root-owned socket under the root-owned run directory.
pub fn request(addresses: [SocketAddr; 2]) -> io::Result<HandedListeners> {
    let uid = nix::unistd::Uid::effective().as_raw();
    authorize(uid, uid, addresses)?;
    let path = socket_path(uid);
    let parent = std::fs::symlink_metadata(Path::new("/private/var/run"))?;
    let metadata = std::fs::symlink_metadata(&path)?;
    if !parent.is_dir()
        || parent.uid() != 0
        || parent.mode() & 0o022 != 0
        || !metadata.file_type().is_socket()
        || metadata.uid() != 0
        || metadata.mode() & 0o007 != 0
    {
        return Err(error(
            "untrusted bind broker socket; run `effigy gateway setup`",
        ));
    }
    let mut stream = UnixStream::connect(path)?;
    if peer_uid(&stream)? != 0 {
        return Err(error("bind broker is not root-owned"));
    }
    receive(&mut stream, addresses)
}
/// Binary entry. The privileged process reads only setup-pinned arguments and
/// launchd's socket, never HOME, routes, certificates, or the kernel Store.
#[cfg(target_os = "macos")]
pub fn entry(uid: u32) -> io::Result<()> {
    use std::os::unix::net::UnixListener;
    if !nix::unistd::Uid::effective().is_root() || uid == 0 {
        return Err(error("invalid broker identity"));
    }
    let fds = listener_handoff::activate_launchd_socket("broker")
        .map_err(|e| error(format!("broker activation failed: {e:?}")))?;
    let owned: Vec<OwnedFd> = fds
        .into_iter()
        .map(|fd| {
            // SAFETY: launch_activate_socket transfers unique descriptor ownership.
            unsafe { OwnedFd::from_raw_fd(fd) }
        })
        .collect();
    let [fd]: [OwnedFd; 1] = owned
        .try_into()
        .map_err(|_| error("expected one broker socket"))?;
    let listener = UnixListener::from(fd);
    if listener.local_addr()?.as_pathname() != Some(socket_path(uid).as_path()) {
        return Err(error("unexpected broker socket"));
    }
    for stream in listener.incoming() {
        let mut stream = stream?;
        // A refused/malformed peer has no effect on other requests.
        let _ = serve_one(&mut stream, uid, TcpListener::bind);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn broker_refuses_wrong_uid_address_range_and_port_before_binding() {
        let good = [
            "127.0.0.1:80".parse().unwrap(),
            "127.1.0.50:443".parse().unwrap(),
        ];
        assert!(authorize(501, 501, good).is_ok());
        assert!(authorize(501, 502, good).is_err());
        for endpoint in [
            "0.0.0.0:80",
            "192.168.0.1:80",
            "127.1.0.51:80",
            "127.2.0.1:80",
            "127.0.0.1:81",
        ] {
            assert!(authorize(501, 501, [endpoint.parse().unwrap(), good[1]]).is_err());
        }
    }
    #[test]
    fn broker_passes_listeners_and_reacquires_after_client_restart() {
        let uid = nix::unistd::Uid::effective().as_raw();
        assert_ne!(uid, 0, "qualification must run as an operator");
        // Ephemeral listeners qualify transport/adoption without installing a
        // privileged job. The production binder is TcpListener::bind and the
        // authorization above always checks the requested low-port endpoints.
        let reserved = [
            TcpListener::bind("127.0.0.1:0").unwrap(),
            TcpListener::bind("127.0.0.1:0").unwrap(),
        ];
        let expected = reserved
            .each_ref()
            .map(|listener| listener.local_addr().unwrap());
        drop(reserved);
        for _ in 0..2 {
            let (mut client, mut broker) = UnixStream::pair().unwrap();
            let thread = std::thread::spawn(move || {
                serve_one(&mut broker, uid, |address| {
                    TcpListener::bind(expected[usize::from(address.port() == 443)])
                })
                .unwrap()
            });
            client
                .write_all(
                    &encode([
                        "127.0.0.1:80".parse().unwrap(),
                        "127.1.0.50:443".parse().unwrap(),
                    ])
                    .unwrap(),
                )
                .unwrap();
            let listeners = receive_reply(&mut client, expected).unwrap();
            thread.join().unwrap();
            for listener in [&listeners.http, &listeners.https] {
                let client = std::net::TcpStream::connect(listener.local_addr().unwrap()).unwrap();
                let (mut accepted, _) = listener.accept().unwrap();
                drop(client);
                assert_eq!(accepted.read(&mut [0]).unwrap(), 0);
                drop(accepted);
            }
        }
    }
    #[test]
    fn broker_foreign_peer_never_reaches_binder() {
        let uid = nix::unistd::Uid::effective().as_raw();
        let (_client, mut broker) = UnixStream::pair().unwrap();
        assert!(serve_one(&mut broker, uid + 1, |_| panic!(
            "foreign peer reached binder"
        ))
        .is_err());
    }
}
