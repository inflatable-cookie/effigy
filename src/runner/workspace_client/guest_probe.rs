//! Context-free staged-object entries. They read no credential, write no
//! journal, and send no workspace-channel frame, including during TLS probes.

use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::time::{Duration, Instant};

use effigy_cli::GuestProbeArgs;
use effigy_kernel::id::Digest;
use effigy_kernel::workspace_channel::Transport;
use serde::{Deserialize, Serialize};

use super::identity::{GuestIdentity, GuestSelfObservation, ProcGuestIdentity};
use crate::runner::error::RunnerError;
use crate::runner::workspace_transport::{opener::resolve_hosts, TlsChannelTransport};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GuestProbeReply {
    pub(crate) schema: String,
    pub(crate) identity: GuestSelfObservation,
    pub(crate) address: Option<IpAddr>,
    pub(crate) peer: Option<SocketAddr>,
    pub(crate) pin: Option<String>,
}

pub(crate) fn run(args: GuestProbeArgs) -> Result<String, RunnerError> {
    let invalid = |message| RunnerError::task_invocation(message);
    let identity = ProcGuestIdentity::process()
        .observe()
        .map_err(|reason| invalid(format!("guest-local identity is unproven ({reason:?})")))?;
    let mut reply = GuestProbeReply {
        schema: "effigy.guest-probe.v1".into(),
        identity,
        address: None,
        peer: None,
        pin: None,
    };
    match args {
        GuestProbeArgs::Identity => {}
        GuestProbeArgs::Resolve => reply.address = Some(address()?),
        GuestProbeArgs::Dial {
            endpoint,
            pin,
            timeout_ms,
        } => {
            let started = Instant::now();
            let timeout = Duration::from_millis(timeout_ms);
            let port = endpoint
                .strip_prefix("effigy-host:")
                .and_then(|port| port.parse::<u16>().ok())
                .filter(|port| *port != 0)
                .ok_or_else(|| {
                    invalid("guest probe requires the prepared alias and assigned port".into())
                })?;
            let pin = pin
                .strip_prefix("sha256:")
                .and_then(Digest::parse_hex)
                .filter(|pin| *pin != Digest([0; 32]))
                .ok_or_else(|| invalid("guest probe requires a canonical known pin".into()))?;
            let address = address()?;
            let remaining = timeout
                .checked_sub(started.elapsed())
                .filter(|remaining| !remaining.is_zero())
                .ok_or_else(|| invalid("guest probe deadline expired".into()))?;
            let peer = SocketAddr::new(address, port);
            let transport = TlsChannelTransport::connect(peer, pin.clone(), remaining)
                .map_err(|error| invalid(format!("guest pinned handshake failed: {error}")))?;
            if transport
                .verified_peer()
                .as_ref()
                .map(|server| &server.digest)
                != Some(&pin)
            {
                return Err(invalid("guest probe peer pin is unverified".into()));
            }
            // The completed TCP connection was made to this exact resolved
            // socket. No resolver, application frame or credential follows it.
            reply.address = Some(address);
            reply.peer = Some(peer);
            reply.pin = Some(format!("sha256:{}", pin.hex()));
            drop(transport);
            if started.elapsed() >= timeout {
                return Err(invalid("guest probe deadline expired".into()));
            }
        }
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
