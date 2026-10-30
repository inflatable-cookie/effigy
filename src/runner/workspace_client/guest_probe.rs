//! Staged-object observation entries. They read no credential, write no
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
        .map_err(|reason| invalid(format!("guest-local identity is unproven ({reason})")))?;
    let mut reply = GuestProbeReply {
        schema: "effigy.guest-probe.v1".into(),
        identity,
        address: None,
        peer: None,
        pin: None,
    };
    let args = match args {
        GuestProbeArgs::ContextDial {
            context,
            digest,
            timeout_ms,
        } => {
            let started = Instant::now();
            let document = read_context(&context, &digest, &reply.identity)?;
            let remaining = Duration::from_millis(timeout_ms)
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| invalid("context probe deadline expired".into()))?;
            GuestProbeArgs::Dial {
                endpoint: document.locator.endpoint.clone(),
                pin: document.locator.pin.clone(),
                timeout_ms: remaining
                    .as_millis()
                    .try_into()
                    .map_err(|_| invalid("context probe budget overflow".into()))?,
            }
        }
        args => args,
    };
    match args {
        GuestProbeArgs::ContextDial { .. } => unreachable!("context dial is resolved above"),
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

/// Integrity plus independently observed placement, never a session-authority
/// proof. The owner separately seals these same bytes against its admitted Run.
fn read_context(
    path: &str,
    expected: &str,
    identity: &GuestSelfObservation,
) -> Result<super::context::v2::WorkspaceContextV2, RunnerError> {
    use super::context::{context_path, WorkspaceContainerIdentity};
    let refused = |message| RunnerError::task_invocation(message);
    let generation = path
        .strip_prefix("/run/effigy/context/")
        .and_then(|tail| tail.strip_suffix("/context.json"))
        .and_then(|number| number.parse::<u64>().ok())
        .filter(|g| *g > 0)
        .filter(|g| context_path(*g) == path)
        .ok_or_else(|| refused("context probe requires an immutable context path".into()))?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(1_048_577).read_to_end(&mut bytes))
        .map_err(|error| refused(format!("context probe read failed: {error}")))?;
    if Digest::parse_hex(expected).as_ref() != Some(&Digest::of(&bytes)) {
        return Err(refused(
            "context probe bytes differ from staged document".into(),
        ));
    }
    let document = super::context::v2::WorkspaceContextV2::parse(&bytes)
        .map_err(|error| refused(error.to_string()))?;
    identity
        .compare(&WorkspaceContainerIdentity {
            container_id: document.container_id.clone(),
            vm_boot_id: document.vm_boot_id.clone(),
            platform: document.platform.clone(),
            libc: document.libc.clone(),
        })
        .map_err(|error| refused(error.to_string()))?;
    if document.context_generation != generation
        || document.version != effigy_core::build_info::active_version()
        || document.protocol != super::context::CHANNEL_PROTOCOL_VERSION
    {
        return Err(refused(
            "context probe version or generation differs".into(),
        ));
    }
    // Hash the executable that is actually running, not a mutable entry link.
    let actual = executable_digest()?;
    if document.artifact_digest != format!("sha256:{}", actual.hex()) {
        return Err(refused(
            "context probe executable differs from pinned artifact".into(),
        ));
    }
    Ok(document)
}
fn executable_digest() -> Result<Digest, RunnerError> {
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
    Ok(Digest(hash.finalize().into()))
}
