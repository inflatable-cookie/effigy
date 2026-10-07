//! macOS process-owned socket inspection via public libproc APIs.
//!
//! Layout constants match `sys/proc_info.h` on the MacOSX SDK used to verify
//! this tree (`PROC_PIDFDSOCKETINFO_SIZE` = 792). A size or flavor mismatch
//! is Unknown; nothing is guessed from `ps`, `lsof`, or SPI sysctl.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use super::legacy::{GatewayEndpoint, GatewayTransport};

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

pub(super) fn process_listening_endpoints(
    pid: u32,
) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
    let pid_t = i32::try_from(pid).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "gateway PID is outside the supported domain",
        )
    })?;
    let fds = list_socket_fds(pid_t)?;
    let mut endpoints = Vec::new();
    for fd in fds {
        match socket_endpoint(pid_t, fd) {
            Ok(Some(endpoint)) => endpoints.push(endpoint),
            Ok(None) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(endpoints)
}

fn list_socket_fds(pid: i32) -> Result<Vec<i32>, std::io::Error> {
    // SAFETY: a null buffer with size 0 is the documented size query for
    // PROC_PIDLISTFDS; pid is a positive signed PID.
    let bytes =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if bytes <= 0 {
        return Err(std::io::Error::last_os_error());
    }
    let count = usize::try_from(bytes).unwrap_or(0) / std::mem::size_of::<libc::proc_fdinfo>();
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut info = vec![
        libc::proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0
        };
        count
    ];
    let want = (count * std::mem::size_of::<libc::proc_fdinfo>()) as i32;
    // SAFETY: `info` is aligned writable storage for `count` proc_fdinfo
    // records, matching the size query above.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            info.as_mut_ptr().cast(),
            want,
        )
    };
    if read <= 0 {
        return Err(std::io::Error::last_os_error());
    }
    let got = usize::try_from(read).unwrap_or(0) / std::mem::size_of::<libc::proc_fdinfo>();
    Ok(info
        .into_iter()
        .take(got)
        .filter(|entry| entry.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
        .map(|entry| entry.proc_fd)
        .collect())
}

fn socket_endpoint(pid: i32, fd: i32) -> Result<Option<GatewayEndpoint>, std::io::Error> {
    let mut buf = [0u8; SOCKET_FDINFO_SIZE];
    // SAFETY: `buf` is aligned writable storage of PROC_PIDFDSOCKETINFO_SIZE
    // bytes; pid/fd are the process-owned descriptors from PROC_PIDLISTFDS.
    let read = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            buf.as_mut_ptr().cast(),
            SOCKET_FDINFO_SIZE as i32,
        )
    };
    if read != SOCKET_FDINFO_SIZE as i32 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "macOS socket_fdinfo size mismatch",
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

#[cfg(test)]
mod macos_socket_layout_tests {
    #[test]
    fn legacy_recovery_macos_socket_fdinfo_matches_sdk_size() {
        assert_eq!(super::SOCKET_FDINFO_SIZE, 792);
        assert_eq!(std::mem::size_of::<libc::proc_fdinfo>(), 8);
    }
}
