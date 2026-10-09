//! Linux process-owned socket inspection via documented procfs interfaces.

use std::collections::BTreeSet;
use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

use super::legacy::{GatewayEndpoint, GatewayTransport};

const TCP_LISTEN: u8 = 0x0A;

pub(super) fn process_listening_endpoints(
    pid: u32,
) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
    let inodes = socket_inodes(pid)?;
    if inodes.is_empty() {
        return Ok(Vec::new());
    }
    let mut endpoints = Vec::new();
    for (table, transport, listen_only) in [
        ("tcp", GatewayTransport::Tcp, true),
        ("tcp6", GatewayTransport::Tcp, true),
        ("udp", GatewayTransport::Udp, false),
        ("udp6", GatewayTransport::Udp, false),
    ] {
        let path = PathBuf::from(format!("/proc/{pid}/net/{table}"));
        let body = fs::read_to_string(&path)?;
        endpoints.extend(parse_proc_net_table(
            &body,
            transport,
            listen_only,
            &inodes,
        )?);
    }
    Ok(endpoints)
}

pub(super) fn verify_process_listener_exclusive(
    pid: u32,
    endpoint: &GatewayEndpoint,
) -> Result<(), std::io::Error> {
    let process_net = fs::metadata(format!("/proc/{pid}/ns/net"))?;
    let current_net = fs::metadata("/proc/self/ns/net")?;
    if process_net.dev() != current_net.dev() || process_net.ino() != current_net.ino() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "managed listener is in a different network namespace",
        ));
    }

    let owned_inodes = socket_inodes(pid)?;
    let mut process_listener_inodes = BTreeSet::new();
    for table in ["tcp", "tcp6"] {
        let body = fs::read_to_string(format!("/proc/{pid}/net/{table}"))?;
        for (current, inode) in parse_proc_net_socket_rows(&body, GatewayTransport::Tcp, true)? {
            if current.addr == endpoint.addr && owned_inodes.contains(&inode) {
                process_listener_inodes.insert(inode);
            }
        }
    }
    if process_listener_inodes.len() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "recorded process does not own exactly one listener socket at {}",
                endpoint.addr
            ),
        ));
    }
    let Some(expected_inode) = process_listener_inodes.first().copied() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "recorded process does not own a listener socket at {}",
                endpoint.addr
            ),
        ));
    };

    let mut matching_inodes = BTreeSet::new();
    for table in ["tcp", "tcp6"] {
        let body = fs::read_to_string(format!("/proc/self/net/{table}"))?;
        for (current, inode) in parse_proc_net_socket_rows(&body, GatewayTransport::Tcp, true)? {
            if endpoints_overlap(&current, endpoint) {
                matching_inodes.insert(inode);
            }
        }
    }
    if matching_inodes.len() != 1 || !matching_inodes.contains(&expected_inode) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "managed endpoint {} has conflicting kernel listener sockets",
                endpoint.addr
            ),
        ));
    }
    Ok(())
}

fn endpoints_overlap(left: &GatewayEndpoint, right: &GatewayEndpoint) -> bool {
    left.transport == right.transport
        && left.addr.port() == right.addr.port()
        && (left.addr.ip() == right.addr.ip()
            || left.addr.ip().is_unspecified()
            || right.addr.ip().is_unspecified())
}

fn socket_inodes(pid: u32) -> Result<BTreeSet<u64>, std::io::Error> {
    let dir = fs::read_dir(format!("/proc/{pid}/fd"))?;
    let mut inodes = BTreeSet::new();
    for entry in dir {
        let entry = entry?;
        let target = match fs::read_link(entry.path()) {
            Ok(target) => target,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let Some(text) = target.to_str() else {
            continue;
        };
        let Some(inode) = text
            .strip_prefix("socket:[")
            .and_then(|rest| rest.strip_suffix(']'))
            .and_then(|inode| inode.parse::<u64>().ok())
        else {
            continue;
        };
        inodes.insert(inode);
    }
    Ok(inodes)
}

fn parse_proc_net_table(
    body: &str,
    transport: GatewayTransport,
    listen_only: bool,
    inodes: &BTreeSet<u64>,
) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
    Ok(parse_proc_net_socket_rows(body, transport, listen_only)?
        .into_iter()
        .filter(|(_, inode)| inodes.contains(inode))
        .map(|(endpoint, _)| endpoint)
        .collect())
}

fn parse_proc_net_socket_rows(
    body: &str,
    transport: GatewayTransport,
    listen_only: bool,
) -> Result<Vec<(GatewayEndpoint, u64)>, std::io::Error> {
    let mut endpoints = Vec::new();
    for line in body.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let _sl = cols.next();
        let Some(local) = cols.next() else {
            continue;
        };
        let _remote = cols.next();
        let Some(st) = cols.next() else {
            continue;
        };
        if listen_only {
            let state = u8::from_str_radix(st, 16).map_err(|_| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Linux socket state is malformed",
                )
            })?;
            if state != TCP_LISTEN {
                continue;
            }
        }
        let inode = cols
            .nth(5)
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Linux socket inode is malformed",
                )
            })?;
        let Some(addr) = parse_proc_net_addr(local) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Linux socket local address is malformed",
            ));
        };
        endpoints.push((GatewayEndpoint { transport, addr }, inode));
    }
    Ok(endpoints)
}

fn parse_proc_net_addr(local: &str) -> Option<SocketAddr> {
    let (ip, port) = local.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let ip = match ip.len() {
        8 => {
            let raw = u32::from_str_radix(ip, 16).ok()?.to_le_bytes();
            IpAddr::V4(Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]))
        }
        32 => {
            let mut raw = [0u8; 16];
            for (index, chunk) in ip.as_bytes().chunks(8).enumerate() {
                let word = std::str::from_utf8(chunk).ok()?;
                let value = u32::from_str_radix(word, 16).ok()?.to_le_bytes();
                let start = index * 4;
                raw[start..start + 4].copy_from_slice(&value);
            }
            IpAddr::V6(Ipv6Addr::from(raw))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod linux_net_parse_tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::net::Ipv4Addr;

    #[test]
    fn legacy_recovery_linux_tcp_listen_and_udp_bind_parse() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 99 1 0000000000000000 100 0 0 10 0\n";
        let udp = "   sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode ref pointer drops\n   38: 0100007F:3C15 00000000:0000 07 00000000:00000000 00:00000000 00000000     0        0 100 2 0000000000000000 0\n";
        let mut inodes = BTreeSet::new();
        inodes.insert(99);
        inodes.insert(100);
        let tcp_eps = parse_proc_net_table(tcp, GatewayTransport::Tcp, true, &inodes).unwrap();
        let udp_eps = parse_proc_net_table(udp, GatewayTransport::Udp, false, &inodes).unwrap();
        assert_eq!(
            tcp_eps,
            vec![GatewayEndpoint {
                transport: GatewayTransport::Tcp,
                addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 80)),
            }]
        );
        assert_eq!(
            udp_eps,
            vec![GatewayEndpoint {
                transport: GatewayTransport::Udp,
                addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 15353)),
            }]
        );
    }

    #[test]
    fn legacy_recovery_linux_tcp_dns_is_not_udp() {
        let tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:3C15 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 7 1 0000000000000000 100 0 0 10 0\n";
        let mut inodes = BTreeSet::new();
        inodes.insert(7);
        let tcp_eps = parse_proc_net_table(tcp, GatewayTransport::Tcp, true, &inodes).unwrap();
        assert_eq!(
            tcp_eps,
            vec![GatewayEndpoint {
                transport: GatewayTransport::Tcp,
                addr: SocketAddr::from((Ipv4Addr::LOCALHOST, 15353)),
            }]
        );
    }

    #[test]
    fn managed_listener_snapshot_keeps_socket_identity_and_rejects_shared_wildcards() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:9C41 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 99 1 0000000000000000 100 0 0 10 0\n   1: 00000000:9C41 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 100 1 0000000000000000 100 0 0 10 0\n";
        let records = parse_proc_net_socket_rows(table, GatewayTransport::Tcp, true).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].1, 99);
        assert_eq!(records[1].1, 100);
        assert!(endpoints_overlap(&records[0].0, &records[1].0));
    }
}
