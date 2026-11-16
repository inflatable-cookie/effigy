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
    let args = match args {
        GuestProbeArgs::ContextDial {
            context,
            digest,
            timeout_ms,
        } => {
            let started = Instant::now();
            let document = read_context(&context, &digest, &reply.identity, &executable)?;
            let remaining = Duration::from_millis(timeout_ms)
                .checked_sub(started.elapsed())
                .filter(|left| !left.is_zero())
                .ok_or_else(|| invalid(REFUSAL_CONTEXT_DEADLINE.into()))?;
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

/// One refusal class a staged guest probe can name.
///
/// The guest writes exactly one fixed sentence per class (the `REFUSAL_*`
/// constants below are their single owner), and the host classifies a failed
/// probe by those prefixes. That keeps a refusal *specific* — "the guest
/// refused the staged context because its version or generation differs" —
/// while the guest's own remaining bytes (paths, digests, the document, io
/// errors) never enter a diagnostic. The guest and the classifier read the
/// same constants, so a sentence cannot be renamed on one side only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeRefusal {
    ContextPath,
    ContextUnreadable,
    ContextBytes,
    ContextDocument,
    ContextIdentity,
    ContextVersion,
    ContextExecutable,
    ContextDeadline,
    IdentityUnproven,
    DialDeadline,
    Unclassified,
}

pub(crate) const REFUSAL_CONTEXT_PATH: &str = "context probe requires an immutable context path";
pub(crate) const REFUSAL_CONTEXT_READ: &str = "context probe read failed";
pub(crate) const REFUSAL_CONTEXT_BYTES: &str = "context probe bytes differ from staged document";
pub(crate) const REFUSAL_CONTEXT_DOCUMENT: &str = "context probe refused the staged document";
pub(crate) const REFUSAL_CONTEXT_IDENTITY: &str = "context probe guest identity differs";
pub(crate) const REFUSAL_CONTEXT_VERSION: &str = "context probe version or generation differs";
pub(crate) const REFUSAL_CONTEXT_EXECUTABLE: &str =
    "context probe executable differs from pinned artifact";
pub(crate) const REFUSAL_CONTEXT_DEADLINE: &str = "context probe deadline expired";
pub(crate) const REFUSAL_IDENTITY_UNPROVEN: &str = "guest-local identity is unproven";
pub(crate) const REFUSAL_DIAL_DEADLINE: &str = "guest probe deadline expired";

/// Every class, in the order the classifier tries them.
const REFUSAL_PREFIXES: &[(&str, ProbeRefusal)] = &[
    (REFUSAL_CONTEXT_PATH, ProbeRefusal::ContextPath),
    (REFUSAL_CONTEXT_READ, ProbeRefusal::ContextUnreadable),
    (REFUSAL_CONTEXT_BYTES, ProbeRefusal::ContextBytes),
    (REFUSAL_CONTEXT_DOCUMENT, ProbeRefusal::ContextDocument),
    (REFUSAL_CONTEXT_IDENTITY, ProbeRefusal::ContextIdentity),
    (REFUSAL_CONTEXT_VERSION, ProbeRefusal::ContextVersion),
    (REFUSAL_CONTEXT_EXECUTABLE, ProbeRefusal::ContextExecutable),
    (REFUSAL_CONTEXT_DEADLINE, ProbeRefusal::ContextDeadline),
    (REFUSAL_IDENTITY_UNPROVEN, ProbeRefusal::IdentityUnproven),
    (REFUSAL_DIAL_DEADLINE, ProbeRefusal::DialDeadline),
];

impl ProbeRefusal {
    /// What a failed staged probe refused, from the guest's own closed
    /// vocabulary. Anything else is `Unclassified`: the host then says only that
    /// the guest refused, never the bytes it printed.
    pub(crate) fn classify(stderr: &[u8]) -> Self {
        for line in String::from_utf8_lossy(stderr).lines() {
            let line = line.trim();
            let line = line.strip_prefix("[error] ").unwrap_or(line);
            for (prefix, class) in REFUSAL_PREFIXES {
                if line.starts_with(prefix) {
                    return *class;
                }
            }
        }
        Self::Unclassified
    }

    /// The host's sentence for this class, or `None` when the guest named no
    /// class the host knows.
    pub(crate) fn sentence(self) -> Option<&'static str> {
        Some(match self {
            Self::ContextPath => {
                "the guest refused the staged context path, which is not an immutable context path"
            }
            Self::ContextUnreadable => "the guest could not read the staged context",
            Self::ContextBytes => {
                "the staged context bytes differ from the document this publication pinned"
            }
            Self::ContextDocument => "the guest refused the staged context document",
            Self::ContextIdentity => "the staged guest's own identity differs from the context",
            Self::ContextVersion => {
                "the guest refused the staged context because its version or generation differs"
            }
            Self::ContextExecutable => "the guest is not running the pinned executable",
            Self::ContextDeadline => "the guest's context probe ran out of time",
            Self::IdentityUnproven => "the guest could not prove its own identity",
            Self::DialDeadline => "the guest's channel dial ran out of time",
            Self::Unclassified => return None,
        })
    }
}

/// Integrity plus independently observed placement, never a session-authority
/// proof. The owner separately seals these same bytes against its admitted Run.
fn read_context(
    path: &str,
    expected: &str,
    identity: &GuestSelfObservation,
    executable: &Digest,
) -> Result<super::context::v2::WorkspaceContextV2, RunnerError> {
    use super::context::{context_path, WorkspaceContainerIdentity};
    let refused = |message| RunnerError::task_invocation(message);
    let generation = path
        .strip_prefix("/run/effigy/context/")
        .and_then(|tail| tail.strip_suffix("/context.json"))
        .and_then(|number| number.parse::<u64>().ok())
        .filter(|g| *g > 0)
        .filter(|g| context_path(*g) == path)
        .ok_or_else(|| refused(REFUSAL_CONTEXT_PATH.into()))?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(1_048_577).read_to_end(&mut bytes))
        .map_err(|error| refused(format!("{REFUSAL_CONTEXT_READ}: {error}")))?;
    if Digest::parse_hex(expected).as_ref() != Some(&Digest::of(&bytes)) {
        return Err(refused(REFUSAL_CONTEXT_BYTES.into()));
    }
    let document = super::context::v2::WorkspaceContextV2::parse(&bytes)
        .map_err(|error| refused(format!("{REFUSAL_CONTEXT_DOCUMENT}: {error}")))?;
    identity
        .compare(&WorkspaceContainerIdentity {
            container_id: document.container_id.clone(),
            vm_boot_id: document.vm_boot_id.clone(),
            platform: document.platform.clone(),
            libc: document.libc.clone(),
        })
        .map_err(|error| refused(format!("{REFUSAL_CONTEXT_IDENTITY}: {error}")))?;
    if document.context_generation != generation
        || document.version != effigy_core::build_info::active_version()
        || document.protocol != super::context::CHANNEL_PROTOCOL_VERSION
    {
        return Err(refused(REFUSAL_CONTEXT_VERSION.into()));
    }
    // The running executable was hashed once for this whole reply; the
    // document must pin those same bytes, not a mutable entry link.
    if document.artifact_digest != format!("sha256:{}", executable.hex()) {
        return Err(refused(REFUSAL_CONTEXT_EXECUTABLE.into()));
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

#[cfg(test)]
mod refusal_tests {
    use super::*;

    #[test]
    fn each_refusal_class_is_recognised_from_the_guest_line() {
        for (line, class, sentence) in [
            (
                "[error] context probe version or generation differs",
                ProbeRefusal::ContextVersion,
                "the guest refused the staged context because its version or generation differs",
            ),
            (
                "[error] context probe bytes differ from staged document",
                ProbeRefusal::ContextBytes,
                "the staged context bytes differ from the document this publication pinned",
            ),
            (
                "[error] context probe read failed: No such file or directory (os error 2)",
                ProbeRefusal::ContextUnreadable,
                "the guest could not read the staged context",
            ),
            (
                "[error] guest-local identity is unproven (boot id missing)",
                ProbeRefusal::IdentityUnproven,
                "the guest could not prove its own identity",
            ),
            (
                "[error] context probe executable differs from pinned artifact",
                ProbeRefusal::ContextExecutable,
                "the guest is not running the pinned executable",
            ),
        ] {
            let classified = ProbeRefusal::classify(line.as_bytes());
            assert_eq!(classified, class, "{line}");
            assert_eq!(classified.sentence(), Some(sentence), "{line}");
        }
    }

    #[test]
    fn an_unrecognised_line_is_unclassified_and_never_echoed() {
        let secret = "[error] adversarial-secret-backend-diagnostic";
        let classified = ProbeRefusal::classify(secret.as_bytes());
        assert_eq!(classified, ProbeRefusal::Unclassified);
        let detail = classified.sentence().unwrap_or("no class").to_owned();
        assert!(!detail.contains("adversarial"), "{detail}");
    }

    #[test]
    fn a_refusal_is_read_from_any_line_of_a_bounded_stream() {
        let stream = b"time=\"...\" level=info msg=starting\n[error] context probe requires an immutable context path\n";
        assert_eq!(ProbeRefusal::classify(stream), ProbeRefusal::ContextPath);
        assert_eq!(ProbeRefusal::classify(b""), ProbeRefusal::Unclassified);
    }
}
