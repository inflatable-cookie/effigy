//! Host ports for bindings that must exist before `up`.
//!
//! Ordinary container ports are assigned by the OS or Compose
//! (`127.0.0.1::<container-port>`) and read back after `up`. A binding whose
//! value must be known before the stack starts — a shared data service, or a
//! service that opted into `stable_ports` — derives its port from a hash of
//! its owner path instead. The value is probed at `up` and never stored, so
//! nothing has to be released.

use sha2::{Digest, Sha256};

/// First port a derived binding may take. Above the ephemeral range's usual
/// low end, below the ports a developer is likely to hard-code.
const STABLE_BASE: u16 = 20000;

/// How many ports above the derived candidate are probed before giving up.
const STABLE_SPAN: u16 = 1000;

/// Lowercase hex SHA-256 digest, used for render digests and evidence.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// A stable, unrecorded host port for `owner`'s `container_port` binding.
///
/// The candidate comes from the owner path and container port, so the same
/// checkout asks for the same port every time. It is probed here: a port this
/// process could not bind is skipped. `taken` carries the ports a caller has
/// already handed out in this plan, so two bindings never collide.
pub fn stable_host_port(owner: &str, container_port: u16, taken: &mut Vec<u16>) -> u16 {
    let digest = Sha256::digest(format!("{owner}\u{0}{container_port}").as_bytes());
    let offset = u16::from_be_bytes([digest[0], digest[1]]) % STABLE_SPAN;
    let start = STABLE_BASE + offset;
    for step in 0..STABLE_SPAN {
        let candidate = STABLE_BASE + (start - STABLE_BASE + step) % STABLE_SPAN;
        if taken.contains(&candidate) || !port_is_free(candidate) {
            continue;
        }
        taken.push(candidate);
        return candidate;
    }
    // Every probed port was busy: fall back to the highest free port the OS
    // gives us inside the same span, so `up` still succeeds.
    for candidate in (STABLE_BASE..STABLE_BASE + STABLE_SPAN).rev() {
        if !taken.contains(&candidate) && port_is_free(candidate) {
            taken.push(candidate);
            return candidate;
        }
    }
    STABLE_BASE
}

/// Whether this process can bind `127.0.0.1:port`.
pub fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// The process holding a listening `127.0.0.1:port`, for a collision message.
/// Best effort: an unreadable or absent inventory yields `None` and the
/// caller still reports the port.
pub fn port_holder(port: u16) -> Option<String> {
    let output = std::process::Command::new("lsof")
        .args(["-nP", "-iTCP", &format!(":{port}"), "-sTCP:LISTEN", "-Fcp"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut command = None;
    let mut pid = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('c') {
            command = Some(value.to_owned());
        } else if let Some(value) = line.strip_prefix('p') {
            pid = Some(value.to_owned());
        }
    }
    match (command, pid) {
        (Some(command), Some(pid)) => Some(format!("{command} (pid {pid})")),
        (Some(command), None) => Some(command),
        _ => None,
    }
}
