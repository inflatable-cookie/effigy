//! Process-owned listening sockets used by managed routes.

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;

/// Transport for one gateway role endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayTransport {
    /// UDP listener (DNS).
    Udp,
    /// TCP listener (HTTP/HTTPS).
    Tcp,
}

/// One process-owned listening endpoint.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GatewayEndpoint {
    /// UDP or TCP.
    pub transport: GatewayTransport,
    /// Local bind address.
    #[serde(
        serialize_with = "serialize_socket_addr",
        deserialize_with = "deserialize_socket_addr"
    )]
    pub addr: SocketAddr,
}

fn serialize_socket_addr<S: serde::Serializer>(
    addr: &SocketAddr,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&addr.to_string())
}

fn deserialize_socket_addr<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<SocketAddr, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

pub fn process_listening_endpoints(pid: u32) -> Result<Vec<GatewayEndpoint>, std::io::Error> {
    #[cfg(target_os = "linux")]
    {
        crate::linux_net::process_listening_endpoints(pid)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos_socket::process_listening_endpoints(pid)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "process socket inspection is unsupported on this platform",
        ))
    }
}

/// Find every same-user process whose TCP listener can accept connections for
/// `endpoint`. A managed route requires exactly one owner; `SO_REUSEPORT` or a
/// wildcard listener shared with a foreign process is refused.
pub fn listening_process_ids(endpoint: &GatewayEndpoint) -> Result<Vec<u32>, std::io::Error> {
    listening_process_ids_for_owner(endpoint, None)
}

/// Listener owners for `endpoint` among processes owned by `owner_uid`, or by
/// the effective user when `None`. A root-owned shared gateway verifies a
/// user's managed listener by scanning that user's processes, not root's.
fn listening_process_ids_for_owner(
    endpoint: &GatewayEndpoint,
    owner_of: Option<u32>,
) -> Result<Vec<u32>, std::io::Error> {
    if endpoint.transport != GatewayTransport::Tcp {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "managed host routes require TCP endpoints",
        ));
    }
    let output = Command::new("ps").args(["-Ao", "pid=,uid="]).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "cannot enumerate local process owners: ps exited with {}",
            output.status
        )));
    }
    let rendered = String::from_utf8(output.stdout).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("local process inventory is not UTF-8: {error}"),
        )
    })?;
    let mut rows = Vec::new();
    for row in rendered.lines() {
        let Some(parsed) = parse_process_inventory_row(row) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "local process inventory contains an invalid PID or UID",
            ));
        };
        rows.push(parsed);
    }
    let scanned_uid = match owner_of {
        Some(expected_pid) => scanned_owner_uid(&rows, expected_pid)?,
        None => nix::unistd::Uid::effective().as_raw(),
    };
    let mut owners = Vec::new();
    for (pid, uid) in rows {
        if pid == 0 || uid != scanned_uid {
            continue;
        }
        let endpoints = match process_listening_endpoints(pid) {
            Ok(endpoints) => endpoints,
            // The process inventory can race with a same-user process exiting.
            // ESRCH proves that PID no longer owns a socket; permission and
            // inspection errors remain fail-closed.
            Err(error) if process_has_disappeared(&error) => continue,
            Err(error) => return Err(error),
        };
        if endpoints
            .iter()
            .any(|current| endpoints_overlap(current, endpoint))
        {
            owners.push(pid);
        }
    }
    owners.sort_unstable();
    owners.dedup();
    Ok(owners)
}

/// Verify that `pid` owns the kernel listener socket and that no second
/// listener socket can accept connections for the same endpoint.
///
/// Linux uses the kernel's socket inode table for the exclusivity check. This
/// avoids requiring permission to inspect unrelated same-user processes while
/// still detecting wildcard and `SO_REUSEPORT` collisions. macOS enumerates
/// listener owners through libproc.
pub fn verify_process_listener_exclusive(
    pid: u32,
    endpoint: &GatewayEndpoint,
) -> Result<(), std::io::Error> {
    if endpoint.transport != GatewayTransport::Tcp {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "managed host routes require TCP endpoints",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        crate::linux_net::verify_process_listener_exclusive(pid, endpoint)
    }
    #[cfg(target_os = "macos")]
    {
        let owners = listening_process_ids_for_owner(endpoint, Some(pid))?;
        if owners.as_slice() == [pid] {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "managed endpoint {} is shared or has unowned listeners: {owners:?}",
                    endpoint.addr
                ),
            ))
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        let _ = endpoint;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "managed listener exclusivity inspection is unsupported on this platform",
        ))
    }
}

/// Parses one `ps -Ao pid=,uid=` row into the PID and the owner UID as the
/// kernel's unsigned `uid_t`.
///
/// `ps` can print a UID in signed 32-bit form: macOS shows some system daemons
/// as `-2`. That presentation is an exact two's-complement encoding of the
/// unsigned UID (`-2` is `4294967294`), so it is mapped to that value and then
/// compared with the effective UID like any other row. A row whose owner is not
/// representable as `i32::MIN..=u32::MAX`, a PID or UID with a sign, non-digit
/// or missing field, and a row with trailing fields are rejected, so the whole
/// inventory fails closed rather than skipping a row it could not classify.
fn parse_process_inventory_row(row: &str) -> Option<(u32, u32)> {
    let mut fields = row.split_whitespace();
    let pid = u32::try_from(parse_ps_integer(fields.next()?, false)?).ok()?;
    let uid = parse_ps_integer(fields.next()?, true)?;
    if fields.next().is_some() {
        return None;
    }
    let owner = if (i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&uid) {
        u32::try_from(uid.rem_euclid(1 << 32)).ok()?
    } else {
        return None;
    };
    Some((pid, owner))
}

/// Parses a `ps` numeric column. Only ASCII digits are accepted, with a leading
/// `-` allowed for the signed UID presentation.
fn parse_ps_integer(token: &str, signed: bool) -> Option<i64> {
    let digits = match token.strip_prefix('-') {
        Some(rest) if signed => rest,
        Some(_) => return None,
        None => token,
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    token.parse::<i64>().ok()
}

/// UID whose processes an exclusivity scan inspects: the expected listener's
/// owner. A non-root gateway may only scan its own user's processes, so a
/// listener owned by anyone else is refused rather than inspected.
fn scanned_owner_uid(rows: &[(u32, u32)], expected_pid: u32) -> Result<u32, std::io::Error> {
    let effective_uid = nix::unistd::Uid::effective().as_raw();
    let Some(&(_, owner_uid)) = rows.iter().find(|(pid, _)| *pid == expected_pid) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("managed listener process {expected_pid} is not running"),
        ));
    };
    if effective_uid != 0 && owner_uid != effective_uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("managed listener process {expected_pid} belongs to another user"),
        ));
    }
    Ok(owner_uid)
}

fn process_has_disappeared(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

fn endpoints_overlap(left: &GatewayEndpoint, right: &GatewayEndpoint) -> bool {
    left.transport == right.transport
        && left.addr.port() == right.addr.port()
        && (left.addr.ip() == right.addr.ip()
            || left.addr.ip().is_unspecified()
            || right.addr.ip().is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The exclusivity scan follows the listener's owner: the caller's own
    /// processes always, and another user's only when the gateway is root.
    #[test]
    fn exclusivity_scan_uses_the_listener_owner_uid() {
        let me = nix::unistd::Uid::effective().as_raw();
        let other = me.wrapping_add(1);
        let rows = [(10, me), (20, other)];
        assert_eq!(scanned_owner_uid(&rows, 10).expect("own listener"), me);
        assert_eq!(
            scanned_owner_uid(&rows, 30)
                .expect_err("missing pid")
                .kind(),
            std::io::ErrorKind::NotFound
        );
        let foreign = scanned_owner_uid(&rows, 20);
        if me == 0 {
            assert_eq!(foreign.expect("root scans the owner's processes"), other);
        } else {
            assert_eq!(
                foreign.expect_err("non-root refuses another user").kind(),
                std::io::ErrorKind::PermissionDenied
            );
        }
    }
    /// Signed `-2` and unsigned `4294967294` are one kernel UID. A row printed
    /// in signed form must compare equal to an effective UID of that value, and
    /// an ordinary different UID must not match it.
    #[test]
    fn process_inventory_row_maps_signed_presentation_to_its_unsigned_uid() {
        let unsigned_minus_two = u32::MAX - 1;
        assert_eq!(
            parse_process_inventory_row("84795    -2"),
            Some((84795, unsigned_minus_two))
        );
        assert_eq!(
            parse_process_inventory_row("3 4294967294"),
            Some((3, unsigned_minus_two))
        );
        assert_eq!(parse_process_inventory_row("5 -1"), Some((5, u32::MAX)));
        assert_eq!(
            parse_process_inventory_row("4 -2147483648"),
            Some((4, 1 << 31))
        );
        assert_eq!(parse_process_inventory_row("1 0"), Some((1, 0)));
        assert_eq!(parse_process_inventory_row("  501 501"), Some((501, 501)));
        assert_eq!(
            parse_process_inventory_row("6 4294967295"),
            Some((6, u32::MAX))
        );
        assert_ne!(
            parse_process_inventory_row("84795 -2").map(|(_, owner)| owner),
            Some(501)
        );
    }
    /// Owners outside `i32::MIN..=u32::MAX` are not UIDs, and rows with missing,
    /// signed-PID, non-digit or trailing fields cannot be classified. None of
    /// them may be skipped as foreign.
    #[test]
    fn process_inventory_row_rejects_out_of_range_and_malformed_rows() {
        for row in [
            "6 4294967296",
            "6 -2147483649",
            "1",
            "",
            "x 501",
            "501 unknown",
            "501 501 extra",
            "-1 501",
            "+1 501",
            "1 +501",
            "1 -",
            "1 1e3",
        ] {
            assert_eq!(parse_process_inventory_row(row), None, "row {row:?}");
        }
    }
    #[test]
    fn process_inventory_row_numeric_columns_accept_only_ascii_digits() {
        assert_eq!(parse_ps_integer("42", false), Some(42));
        assert_eq!(parse_ps_integer("-42", true), Some(-42));
        assert_eq!(parse_ps_integer("-42", false), None);
        assert_eq!(parse_ps_integer("", true), None);
        assert_eq!(parse_ps_integer("٤٢", false), None);
    }
}
