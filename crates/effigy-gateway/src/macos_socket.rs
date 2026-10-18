//! macOS process-owned socket inspection via public libproc APIs.
//!
//! Layout constants match `sys/proc_info.h` on the MacOSX SDK used to verify
//! this tree (`PROC_PIDFDSOCKETINFO_SIZE` = 792). A size or flavor mismatch
//! is Unknown; nothing is guessed from `ps`, `lsof`, or SPI sysctl.
//! Return-value contracts: Apple XNU f6217f891ac0bb64f3d375211650a4c1ff8ca1ea,
//! bsd/kern/proc_info.c::proc_pidfdlist and
//! libsyscall/wrappers/libproc/libproc.c::{proc_pidinfo, proc_pidfdinfo}.

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

/// Offset of `soi_so` (the kernel's opaque socket handle) inside
/// `socket_fdinfo`: `sizeof(proc_fileinfo)` + `offsetof(socket_info, soi_so)`
/// (= `sizeof(vinfo_stat)` = 136). Two descriptors of one socket share the
/// handle; macOS `fstat` reports dev=0/ino=0 for every socket and cannot.
const SOI_SO: usize = PSI_OFFSET + 136;

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
            // `PROC_PIDLISTFDS` is a snapshot. A non-listening connection may
            // close before its descriptor is inspected; those vanished file
            // descriptors no longer own a listener and can be skipped.
            Err(error)
                if error.raw_os_error() == Some(libc::EBADF)
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                continue;
            }
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
    // libproc maps errors to zero, but a successful live zero-FD list also
    // returns zero. Clear this thread's errno before the ambiguous data query.
    nix::errno::Errno::clear();
    // SAFETY: `info` is aligned writable storage for `count` proc_fdinfo
    // records in one initialized allocation, matching the positive size query
    // above. The byte extent fits i32; the synchronous call retains no pointer.
    let read = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            info.as_mut_ptr().cast(),
            want,
        )
    };
    let errno = nix::errno::Errno::last_raw();
    let got = fd_list_record_count(read, errno, want as usize)?;
    Ok(info
        .into_iter()
        .take(got)
        .filter(|entry| entry.proc_fdtype == libc::PROX_FDTYPE_SOCKET as u32)
        .map(|entry| entry.proc_fd)
        .collect())
}

fn fd_list_record_count(read: i32, errno: i32, capacity: usize) -> std::io::Result<usize> {
    if read < 0 || (read == 0 && errno != 0) {
        return Err(libproc_error(errno));
    }
    let bytes = read as usize;
    let record_size = std::mem::size_of::<libc::proc_fdinfo>();
    if bytes > capacity || !bytes.is_multiple_of(record_size) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "macOS descriptor list has an invalid record extent",
        ));
    }
    Ok(bytes / record_size)
}

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
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);

    impl Drop for OwnedChild {
        fn drop(&mut self) {
            // Keep this exact child unreaped until its fixture is finished.
            // Never signal a PID discovered by an observation or name lookup.
            let _ = self.0.kill();
            self.0.wait().expect("reap owned descriptorless fixture");
        }
    }

    #[test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known v0.15 rebuild defect: papercut b7c0ef8d-78f4-4894-ba6d-dae46aa58a91; owner lane v015-gateway"
    )]
    fn legacy_recovery_macos_live_descriptorless_child_has_no_listeners() {
        let child = OwnedChild(
            Command::new("/bin/sh")
                .args(["-c", "exec 0<&- 1>&- 2>&-; exec /bin/sleep 30"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn owned descriptorless fixture"),
        );
        let pid = i32::try_from(child.0.id()).expect("fixture PID");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let mut info = [libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0,
            }; 64];
            nix::errno::Errno::clear();
            // SAFETY: pid belongs to our retained, unreaped Child. The aligned
            // initialized array remains writable for exactly the byte size
            // passed to the documented PROC_PIDLISTFDS data query. No borrowed
            // pointer escapes the synchronous call. The native zero/errno=0
            // observation proves this fixture reached its descriptorless state.
            let read = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDLISTFDS,
                    0,
                    info.as_mut_ptr().cast(),
                    std::mem::size_of_val(&info) as i32,
                )
            };
            let errno = std::io::Error::last_os_error().raw_os_error();
            if read == 0 && errno == Some(0) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "fixture never became descriptorless"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        // Deliberately dirty errno: successful empty lists must not inherit it.
        nix::errno::Errno::EACCES.set();
        let result = super::process_listening_endpoints(child.0.id());
        let listener = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("private listener for production exclusivity scan");
        let endpoint = super::GatewayEndpoint {
            transport: super::GatewayTransport::Tcp,
            addr: listener.local_addr().expect("private listener address"),
        };
        let exclusive =
            crate::legacy::verify_process_listener_exclusive(std::process::id(), &endpoint);
        drop(child);
        assert!(result
            .expect("live zero-FD process is inspectable")
            .is_empty());
        exclusive.expect("descriptorless sibling must not poison the exclusive listener proof");
    }

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
    fn legacy_recovery_macos_fd_list_distinguishes_empty_errors_and_invalid_extents() {
        let record = std::mem::size_of::<libc::proc_fdinfo>();
        assert_eq!(super::fd_list_record_count(0, 0, record).unwrap(), 0);
        assert_eq!(
            super::fd_list_record_count(record as i32, libc::EACCES, record).unwrap(),
            1
        );
        for errno in [libc::EPERM, libc::EACCES, libc::ESRCH, libc::EBADF] {
            let error = super::fd_list_record_count(0, errno, record).unwrap_err();
            assert_eq!(error.raw_os_error(), Some(errno));
        }
        for (read, capacity) in [(-1, record), (1, record), ((record * 2) as i32, record)] {
            assert_eq!(
                super::fd_list_record_count(read, 0, capacity)
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::InvalidData,
            );
        }
    }

    #[test]
    fn legacy_recovery_macos_socket_fdinfo_matches_sdk_size() {
        assert_eq!(super::SOCKET_FDINFO_SIZE, 792);
        assert_eq!(std::mem::size_of::<libc::proc_fdinfo>(), 8);
        // `soi_so` sits after `proc_fileinfo` (24) and `vinfo_stat` (136);
        // the socket handle must land inside the fixed prefix of the record.
        assert_eq!(super::SOI_SO, 160);
        const { assert!(super::SOI_SO + 8 <= super::SOCKET_FDINFO_SIZE) };
    }
}
