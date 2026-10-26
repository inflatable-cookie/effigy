//! macOS process-owned socket inspection via public libproc APIs.
//!
//! Layout constants match `sys/proc_info.h` on the MacOSX SDK used to verify
//! this tree (`PROC_PIDFDSOCKETINFO_SIZE` = 792). A size or flavor mismatch
//! is Unknown; nothing is guessed from `ps`, `lsof`, or SPI sysctl.
//! Return-value contracts: Apple XNU f6217f891ac0bb64f3d375211650a4c1ff8ca1ea,
//! bsd/kern/proc_info.c::proc_pidfdlist and
//! libsyscall/wrappers/libproc/libproc.c::{proc_pidinfo, proc_pidfdinfo}.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::socket_ownership::{GatewayEndpoint, GatewayTransport};

/// `PROC_PIDFDSOCKETINFO` from public `sys/proc_info.h`.
const PROC_PIDFDSOCKETINFO: i32 = 3;
/// Verified `sizeof(struct socket_fdinfo)` / `PROC_PIDFDSOCKETINFO_SIZE`.
const SOCKET_FDINFO_SIZE: usize = 792;

const PSI_OFFSET: usize = 24;
const SOI_PROTOCOL: usize = PSI_OFFSET + 156;
const SOI_FAMILY: usize = PSI_OFFSET + 160;
const SOI_KIND: usize = PSI_OFFSET + 232;
const SOI_PROTO: usize = PSI_OFFSET + 240;
const INSI_LPORT: usize = 4;
const INSI_VFLAG: usize = 24;
const INSI_LADDR4: usize = 48 + 12;
const INSI_LADDR6: usize = 48;
const TCPSI_STATE: usize = 80;

const SOCKINFO_IN: i32 = 1;
const SOCKINFO_TCP: i32 = 2;
const TSI_S_LISTEN: i32 = 1;
const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;

/// Offset of `soi_so` (the kernel's opaque socket handle) inside
/// `socket_fdinfo`: `sizeof(proc_fileinfo)` + `offsetof(socket_info, soi_so)`
/// (= `sizeof(vinfo_stat)` = 136). Two descriptors of one socket share the
/// handle; macOS `fstat` reports dev=0/ino=0 for every socket and cannot.
const SOI_SO: usize = PSI_OFFSET + 136;

fn libproc_error(errno: i32) -> std::io::Error {
    if errno == 0 {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "macOS process inspection returned no record without an error code",
        )
    } else {
        std::io::Error::from_raw_os_error(errno)
    }
}

/// Kernel socket handle for one process-owned descriptor.
///
/// The listener handoff uses this to refuse duplicate socket objects: two
/// descriptors of the same socket share the handle, while macOS `fstat`
/// reports `dev=0, ino=0` for every socket and cannot distinguish them.
pub(super) fn fd_socket_handle(pid: i32, fd: i32) -> Result<u64, std::io::Error> {
    let mut buf = [0u8; SOCKET_FDINFO_SIZE];
    nix::errno::Errno::clear();
    // SAFETY: `buf` is one initialized writable allocation of exactly the
    // public PROC_PIDFDSOCKETINFO_SIZE. The synchronous copy retains no pointer;
    // invalid or vanished pid/fd values are native errors, never dereferenced.
    let read = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            buf.as_mut_ptr().cast(),
            SOCKET_FDINFO_SIZE as i32,
        )
    };
    let errno = nix::errno::Errno::last_raw();
    if read <= 0 {
        return Err(libproc_error(errno));
    }
    if read as usize != SOCKET_FDINFO_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "macOS socket_fdinfo size mismatch: expected {SOCKET_FDINFO_SIZE} bytes, got {read}"
            ),
        ));
    }
    read_u64_slice(&buf, SOI_SO)
}

/// Listening-endpoint probe for one process-owned descriptor, used by the
/// listener handoff to verify a handed socket is in the listening state
/// (macOS has no `SO_ACCEPTCONN`; this is the documented libproc probe).
pub(super) fn socket_endpoint(
    pid: i32,
    fd: i32,
) -> Result<Option<GatewayEndpoint>, std::io::Error> {
    let mut buf = [0u8; SOCKET_FDINFO_SIZE];
    nix::errno::Errno::clear();
    // SAFETY: `buf` is one initialized writable allocation of exactly the
    // public PROC_PIDFDSOCKETINFO_SIZE. The synchronous copy retains no pointer;
    // invalid or vanished pid/fd values are native errors, never dereferenced.
    let read = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            buf.as_mut_ptr().cast(),
            SOCKET_FDINFO_SIZE as i32,
        )
    };
    let errno = nix::errno::Errno::last_raw();
    // Unlike a variable-length FD list, a successful socket query supplies
    // one complete record. libproc maps native errors (including permissions)
    // to zero; only the caller's exact EBADF/ESRCH policy may skip a race.
    if read <= 0 {
        return Err(libproc_error(errno));
    }
    if read != SOCKET_FDINFO_SIZE as i32 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "macOS socket_fdinfo size mismatch: expected {SOCKET_FDINFO_SIZE} bytes, got {read}"
            ),
        ));
    }
    parse_socket_fdinfo(&buf)
}

fn parse_socket_fdinfo(
    buf: &[u8; SOCKET_FDINFO_SIZE],
) -> Result<Option<GatewayEndpoint>, std::io::Error> {
    let kind = read_i32(buf, SOI_KIND)?;
    let protocol = read_i32(buf, SOI_PROTOCOL)?;
    let family = read_i32(buf, SOI_FAMILY)?;
    let proto = &buf[SOI_PROTO..];
    let (transport, listening) = match kind {
        SOCKINFO_TCP => {
            let state = read_i32_slice(proto, TCPSI_STATE)?;
            (GatewayTransport::Tcp, state == TSI_S_LISTEN)
        }
        SOCKINFO_IN if protocol == libc::IPPROTO_UDP => (GatewayTransport::Udp, true),
        _ => return Ok(None),
    };
    if !listening {
        return Ok(None);
    }
    let vflag = proto.get(INSI_VFLAG).copied().unwrap_or(0);
    let port = read_i32_slice(proto, INSI_LPORT)? as u16;
    let port = u16::from_be(port);
    let ip = if vflag & INI_IPV4 != 0 || family == libc::AF_INET {
        let octets: [u8; 4] = proto
            .get(INSI_LADDR4..INSI_LADDR4 + 4)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "IPv4 address missing")
            })?
            .try_into()
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "IPv4 address missing")
            })?;
        IpAddr::V4(Ipv4Addr::from(octets))
    } else if vflag & INI_IPV6 != 0 || family == libc::AF_INET6 {
        let octets: [u8; 16] = proto
            .get(INSI_LADDR6..INSI_LADDR6 + 16)
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "IPv6 address missing")
            })?
            .try_into()
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "IPv6 address missing")
            })?;
        IpAddr::V6(Ipv6Addr::from(octets))
    } else {
        return Ok(None);
    };
    Ok(Some(GatewayEndpoint {
        transport,
        addr: SocketAddr::new(ip, port),
    }))
}

fn read_i32(buf: &[u8], offset: usize) -> Result<i32, std::io::Error> {
    read_i32_slice(buf, offset)
}

fn read_i32_slice(buf: &[u8], offset: usize) -> Result<i32, std::io::Error> {
    let bytes = buf.get(offset..offset + 4).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "socket_fdinfo field is out of range",
        )
    })?;
    Ok(i32::from_ne_bytes(bytes.try_into().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "socket_fdinfo field is out of range",
        )
    })?))
}

fn read_u64_slice(buf: &[u8], offset: usize) -> Result<u64, std::io::Error> {
    let bytes = buf.get(offset..offset + 8).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "socket_fdinfo field is out of range",
        )
    })?;
    Ok(u64::from_ne_bytes(bytes.try_into().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "socket_fdinfo field is out of range",
        )
    })?))
}

#[cfg(test)]
mod macos_socket_layout_tests {
    #[test]
    fn legacy_recovery_macos_invalid_own_socket_descriptor_preserves_native_error() {
        let pid = i32::try_from(std::process::id()).expect("current PID");
        let error = super::socket_endpoint(pid, i32::MAX)
            .expect_err("libproc errors are not evidence of an absent listener");
        assert_eq!(error.raw_os_error(), Some(libc::EBADF));
        let error = super::fd_socket_handle(pid, i32::MAX)
            .expect_err("socket-handle query preserves native refusal");
        assert_eq!(error.raw_os_error(), Some(libc::EBADF));
    }

    #[test]
    fn legacy_recovery_macos_socket_fdinfo_matches_sdk_size() {
        assert_eq!(super::SOCKET_FDINFO_SIZE, 792);
        // `soi_so` sits after `proc_fileinfo` (24) and `vinfo_stat` (136);
        // the socket handle must land inside the fixed prefix of the record.
        assert_eq!(super::SOI_SO, 160);
        const { assert!(super::SOI_SO + 8 <= super::SOCKET_FDINFO_SIZE) };
    }
}
