//! Staged-object observation entries. They read no credential and write no
//! journal: a probe answers with the guest's own identity and, on request, the
//! loopback address its prepared alias resolves to.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};

use effigy_cli::GuestProbeArgs;
use serde::{Deserialize, Serialize};

use super::identity::{GuestIdentity, GuestSelfObservation, ProcGuestIdentity};
use crate::runner::error::RunnerError;

pub(crate) const REFUSAL_IDENTITY_UNPROVEN: &str = "guest-local identity is unproven";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuestProbeReply {
    pub(crate) schema: String,
    pub(crate) identity: GuestSelfObservation,
    /// SHA-256 of the bytes this process is actually running (`/proc/self/exe`).
    /// The host refuses an observation that does not name the pinned object, so
    /// a replaced object cannot report evidence for the object it replaced.
    #[serde(default)]
    pub(crate) executable: Option<String>,
    pub(crate) address: Option<IpAddr>,
    pub(crate) peer: Option<SocketAddr>,
    pub(crate) pin: Option<String>,
}

pub(crate) fn run(args: GuestProbeArgs) -> Result<String, RunnerError> {
    let invalid = |message| RunnerError::task_invocation(message);
    let identity = ProcGuestIdentity::process()
        .observe()
        .map_err(|reason| invalid(format!("{REFUSAL_IDENTITY_UNPROVEN} ({reason})")))?;
    // Hash the running executable once, before any observation, so every reply
    // names the exact bytes that produced it.
    let executable = executable_digest()?;
    let mut reply = GuestProbeReply {
        schema: "effigy.guest-probe.v1".into(),
        identity,
        executable: Some(format!("sha256:{}", executable.hex())),
        address: None,
        peer: None,
        pin: None,
    };
    match args {
        GuestProbeArgs::Identity => {}
        GuestProbeArgs::Resolve => reply.address = Some(address()?),
    }
    serde_json::to_string(&reply)
        .map_err(|error| invalid(format!("cannot encode guest observation: {error}")))
}

fn address() -> Result<IpAddr, RunnerError> {
    let mut bytes = Vec::new();
    std::fs::File::open("/etc/hosts")
        .and_then(|file| file.take(65537).read_to_end(&mut bytes))
        .map_err(|error| {
            RunnerError::task_invocation(format!("guest alias observation failed: {error}"))
        })?;
    if bytes.len() > 65536 {
        return Err(RunnerError::task_invocation(
            "guest hosts file exceeds its read budget",
        ));
    }
    resolve_hosts(&bytes)
        .map_err(|error| RunnerError::task_invocation(format!("guest alias is unproven: {error}")))
}

/// Resolve the one prepared alias from the guest's own hosts file. A loopback or
/// unspecified answer is unproven, never a host address.
fn resolve_hosts(hosts: &[u8]) -> Result<IpAddr, String> {
    let mut resolved = None;
    for line in String::from_utf8_lossy(hosts).lines() {
        let line = line.split('#').next().unwrap_or_default().trim();
        let mut fields = line.split_whitespace();
        let Some(address) = fields.next() else {
            continue;
        };
        if !fields.any(|name| name == "effigy-host") {
            continue;
        }
        let address: IpAddr = address
            .parse()
            .map_err(|_| "guest alias is not an address".to_owned())?;
        if address.is_loopback() || address.is_unspecified() {
            return Err("guest alias is not a routable address".into());
        }
        resolved = Some(address);
    }
    resolved.ok_or_else(|| "guest alias is absent".into())
}

fn executable_digest() -> Result<effigy_kernel::id::Digest, RunnerError> {
    use sha2::Digest as _;
    let mut file = std::fs::File::open("/proc/self/exe")
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut hash = sha2::Sha256::new();
    let mut buffer = [0; 65536];
    let mut total = 0usize;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        if count == 0 {
            break;
        }
        total = total
            .checked_add(count)
            .filter(|size| *size <= 512 * 1024 * 1024)
            .ok_or_else(|| RunnerError::task_invocation("guest executable exceeds probe budget"))?;
        hash.update(&buffer[..count]);
    }
    Ok(effigy_kernel::id::Digest(hash.finalize().into()))
}

#[cfg(test)]
mod probe_tests {
    use super::*;

    #[test]
    fn a_loopback_alias_is_unproven() {
        assert!(resolve_hosts(b"127.0.0.1 effigy-host\n").is_err());
        assert!(resolve_hosts(b"").is_err());
        assert_eq!(
            resolve_hosts(b"192.168.99.100 effigy-host\n").expect("alias"),
            "192.168.99.100".parse::<IpAddr>().expect("address")
        );
    }

    #[test]
    fn an_unproven_identity_names_the_sentence_the_guest_prints() {
        assert_eq!(
            REFUSAL_IDENTITY_UNPROVEN,
            "guest-local identity is unproven"
        );
    }
}
