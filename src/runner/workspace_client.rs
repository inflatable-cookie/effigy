//! Container-mode workspace client: the guest side of contract 052 §8.11.
//!
//! A container-mode workspace publishes one immutable context (lane C,
//! [`context`]). A guest Effigy reads that context, serves its static reads
//! from it, and sends everything with host authority over the one
//! authenticated, per-session channel lane A owns
//! ([`effigy_kernel::workspace_channel`]). This module is the only place those
//! lanes meet (contract 052 §8.11, "Owners and coverage"), and it holds no
//! authority of its own: no store, no gate, no run, no resolution and no
//! second credential. A host refusal is final — nothing here falls back to
//! guest state or guest execution (§8.10 R1).
//!
//! What this file deliberately does not own:
//!
//! - The materialised context, the pinned snapshots and guest placement
//!   detection belong to lane C ([`context`], `workspace_command::provisioning`).
//! - The wire, framing, negotiation, grants and the host dispatcher belong to
//!   lane A (`crates/effigy-kernel/src/workspace_channel.rs`).
//! - The workspace session, its attachments and the down coordination belong
//!   to lane B (`crates/effigy-kernel/src/workspace_session.rs`); the guest
//!   only ever names a session id it was given.
//! - Resolution, admission, observation and effect recording happen on the
//!   host, in the owning engines the dispatcher delegates to.
//!
//! The transport is not here either: a context pins a channel locator only
//! once a transport is qualified for its backend, and until then the locator
//! is absent and host access refuses with [`RunnerError`] naming the reason.
//!
//! Three facts the merged lanes do not yet give a guest are refused for,
//! never guessed at, and each refusal names what is missing:
//!
//! 1. A guest-local observation of the container identity (container id, VM
//!    boot, platform, libc). The merged probing seam is host-driven Prepare
//!    ([`context::GuestWorkspace::probe_container_identity`]); nothing in the
//!    guest can establish [`context::LiveGuestRun`] without trusting the
//!    context's own claim, which §8.1 forbids.
//! 2. A qualified channel transport for the backend in use, so no context
//!    published today carries a ready [`context::WorkspaceChannelLocator`].
//! 3. The session generation a direct terminal must present when it enrols
//!    (lane A's `EnrolRequest::session_generation`). The published context
//!    names the session run and not its generation.

// The channel client below is exercised by the proofs in `tests.rs` and is
// opened by the transport integration that is a separate task (lane A's note
// in contract 052 §8.11: "The transport, the guest client and the runner
// integration remain separate tasks"). Until that transport lands, nothing in
// this crate constructs a `WorkspaceClient` outside those proofs, so the
// client's items are dead code in a product build — the same, temporary shape
// lane C's `context.rs` records for its verification seam.
#![allow(dead_code)]

pub(crate) mod context;
// #482's producer side: the staged guest probe and the guest's own identity
// observation, which the v2 context verification takes.
pub(crate) mod guest_probe;
pub(crate) mod identity;
pub(crate) mod serving;
pub(crate) mod serving_facts;

pub(crate) use serving_facts::ServingFacts;

#[cfg(test)]
pub(crate) mod guest_fake;

use effigy_cli::Command;
use effigy_kernel::id::{Digest, Generation, RunId};
use effigy_kernel::run::RequestId;
use effigy_kernel::workspace_channel::{
    self as channel, AttachBody, EnrolRequest, ExitStatus, Frame, Hello, InvocationBody,
    ObserveBody, OutputMode as ChannelOutputMode, Request, RequestBody, Secret, Selector,
    StreamKind, Target, Transport,
};
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;

use context::v2::{PublishedGuestContext, WorkspaceContextV2};
use context::{
    CheckoutReader, ContextReadError, ContextRefusal, GuestLocalReads, GuestOperationClass,
    GuestReader, LiveGuestRun, LocalRead, LocalReadError, LocalReadOutcome,
    VerifiedWorkspaceContext, WorkspaceContainerIdentity, WorkspacePathMap, CLIENT_ROOT,
    ENROL_CREDENTIAL_PATH, HOST_REPREPARE_COMMAND,
};
use identity::{GuestIdentity, GuestSelfObservation, ProcGuestIdentity};

/// How long a guest may spend opening the pinned TLS connection. Bounded so a
/// wedged host never keeps a command alive.
const CHANNEL_OPEN_BUDGET: std::time::Duration = std::time::Duration::from_secs(5);

/// The guest-private store lane C publishes under. A local read is confined to
/// it, so a pinned snapshot read can never become a general host read.
const GUEST_STORE_ROOT: &str = "/run/effigy/";

/// The one capability a guest stores for itself after an entry hands it over
/// (§8.8). It is guest-private, never in the checkout and never in a frame.
fn client_capability_path() -> String {
    format!("{CLIENT_ROOT}/capability")
}

/// Where this Effigy is running.
#[derive(Debug)]
pub(crate) enum GuestPlacement {
    /// No context is published: the ordinary host dispatch runs unchanged.
    /// Hybrid mode always lands here, because nothing publishes a context for
    /// it and this path writes nothing.
    Host,
    /// The workspace is in container mode and prepared for one generation,
    /// already matched against this guest's own independent observation.
    Guest(Box<PublishedGuestContext>),
}

/// Read the published context, or report that this is not a prepared
/// container-mode guest. This is the only placement test: a context the host
/// published for this container, not a hostname, address or marker file
/// (§8.1).
pub(crate) fn detect(guest: &mut dyn GuestReader) -> Result<GuestPlacement, RunnerError> {
    detect_with(guest, &mut ProcGuestIdentity::process())
}

/// [`detect`] with the guest's own identity observation supplied.
///
/// The strict v2 document is selected only after the observation independently
/// matches the container it claims to be; a v1 document is refused by the v2
/// parser rather than reinterpreted, and an unmatched observation refuses
/// before any local read or connection.
pub(crate) fn detect_with(
    guest: &mut dyn GuestReader,
    identity: &mut dyn GuestIdentity,
) -> Result<GuestPlacement, RunnerError> {
    match context::v2::read_published_guest(guest, identity) {
        Ok(Some(published)) => Ok(GuestPlacement::Guest(Box::new(published))),
        Ok(None) => Ok(GuestPlacement::Host),
        Err(error) => Err(context_read_error(error)),
    }
}

/// The container's own filesystem. Guest paths are absolute inside the
/// container; this reader is confined to the guest store lane C publishes.
#[derive(Debug, Default)]
pub(crate) struct ContainerGuest;

impl GuestReader for ContainerGuest {
    fn read_file(&mut self, path: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        if !path.starts_with(GUEST_STORE_ROOT) {
            return Err(RunnerError::task_invocation(format!(
                "`{path}` is outside the guest store `{GUEST_STORE_ROOT}`; a container-mode guest \
                 reads only what the host prepared"
            )));
        }
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "cannot read `{path}` in the workspace container: {error}; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
            ))),
        }
    }

    fn file_sha256(&mut self, path: &str) -> Result<Option<String>, RunnerError> {
        Ok(self
            .read_file(path)?
            .map(|bytes| context::sha256_text(&bytes)))
    }
}

/// The mounted checkout, restricted to its guest root. A pinned read may name
/// a file inside the checkout, never an absolute path, a `..` component or a
/// symlink that leaves the mount (§8.2).
#[derive(Debug)]
pub(crate) struct MountedCheckout {
    root: PathBuf,
}

impl MountedCheckout {
    pub(crate) fn new(paths: &WorkspacePathMap) -> Self {
        Self {
            root: PathBuf::from(&paths.guest_checkout),
        }
    }
}

impl CheckoutReader for MountedCheckout {
    fn read_checkout_file(&mut self, relative: &str) -> Result<Option<Vec<u8>>, RunnerError> {
        if relative.starts_with('/') || relative.contains(['\\', '\0']) {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` is not a checkout-relative path"
            )));
        }
        if relative.split('/').any(|part| part == "..") {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` traverses above the mounted checkout"
            )));
        }
        let root = std::fs::canonicalize(&self.root).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot resolve the mounted checkout `{}`: {error}; run \
                 `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                self.root.display()
            ))
        })?;
        let resolved = match std::fs::canonicalize(root.join(relative)) {
            Ok(resolved) => resolved,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(RunnerError::task_invocation(format!(
                    "cannot read `{relative}` from the mounted checkout: {error}"
                )))
            }
        };
        if !resolved.starts_with(&root) {
            return Err(RunnerError::task_invocation(format!(
                "`{relative}` resolves outside the mounted checkout `{}`",
                self.root.display()
            )));
        }
        match std::fs::read(&resolved) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(RunnerError::task_invocation(format!(
                "cannot read `{relative}` from the mounted checkout: {error}"
            ))),
        }
    }
}

/// What one guest command is, per §8.6's coverage table.
///
/// A `Local` read is answered from the pinned context and the mounted checkout
/// and never opens a connection. `Observe` and `Invoke` both cross the one
/// channel; the host decides the real effect class from its own policy, never
/// from this label (§8.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GuestOperation {
    Local(LocalRead),
    Observe {
        op: String,
        target: Target,
    },
    Invoke {
        selector: Selector,
        target: Target,
    },
    /// Refused in the guest before anything crosses a connection.
    Refused(String),
}

/// Classify one command for a container-mode guest.
///
/// The guest serves the pinned reads, asks the host for live facts, and asks
/// the host to resolve everything with authority. Static declarations the
/// context already carries stay local; the gateway never runs in a guest and
/// hidden internal entry points are refused outright (§8.6).
pub(crate) fn classify(cmd: &Command) -> GuestOperation {
    use effigy_cli::{ContainerSubcommand, RunsAction};

    let operation = |label: &str| GuestOperation::Invoke {
        selector: Selector::Operation(label.to_owned()),
        target: Target::Host,
    };
    let observation = |label: &str| GuestOperation::Observe {
        op: label.to_owned(),
        target: Target::Host,
    };
    match cmd {
        // The pinned identity this guest runs under, and the task names the
        // context's composed manifest snapshot carries.
        Command::Version => GuestOperation::Local(LocalRead::Version),
        Command::Tasks(args) => {
            if args.qa.is_some() {
                operation("tasks:qa-group")
            } else if args.status_selector.is_some() || args.status_all {
                observation("tasks:status")
            } else if args.resolve_selector.is_some() || args.task_name.is_some() {
                observation("tasks:show")
            } else {
                GuestOperation::Local(LocalRead::Tasks)
            }
        }
        // A task is an admission: the host resolves, gates and journals it.
        Command::Task(task) => GuestOperation::Invoke {
            selector: Selector::Task(task.name.clone()),
            target: Target::Host,
        },
        Command::Container(args) => match &args.subcommand {
            ContainerSubcommand::Status { .. } => observation("container:status"),
            ContainerSubcommand::Stats { .. } => observation("container:stats"),
            ContainerSubcommand::Logs { .. } => observation("container:logs"),
            ContainerSubcommand::Render { .. } => observation("container:render"),
            ContainerSubcommand::Scope => observation("container:scope"),
            ContainerSubcommand::Hosts { .. } => observation("container:hosts"),
            ContainerSubcommand::Up { .. } => operation("container:up"),
            // `container down` from inside is allowed: the host checks its own
            // holders and tells the terminal before the container stops (§8.7).
            ContainerSubcommand::Down { .. } => operation("container:down"),
            ContainerSubcommand::Restart { .. } => operation("container:restart"),
            ContainerSubcommand::Shell { .. } => operation("container:shell"),
            ContainerSubcommand::Reset { .. } => operation("container:reset"),
            ContainerSubcommand::Cache { .. } => operation("container:cache"),
            ContainerSubcommand::Volume { .. } => operation("container:volume"),
            ContainerSubcommand::Profile { .. } => operation("container:profile"),
            ContainerSubcommand::Data { .. } => operation("container:data"),
            ContainerSubcommand::Migrate { .. } => operation("container:migrate"),
            ContainerSubcommand::Retire { .. } => operation("container:retire"),
        },
        // The workspace entry attaches to the one session run (§8.7).
        Command::Workspace(_) => operation("workspace:attach"),
        Command::Doctor(_) => observation("doctor"),
        Command::Runs(args) => match &args.action {
            RunsAction::List { .. } => observation("runs:list"),
            RunsAction::Status { .. } => observation("runs:status"),
            RunsAction::Logs { .. } => observation("runs:logs"),
            RunsAction::Stop { .. } => operation("runs:stop"),
        },
        Command::Bundle(_) => operation("bundle"),
        Command::Changelog(_) => operation("changelog"),
        Command::Fmt(_) => operation("fmt"),
        Command::Deploy(_) => operation("deploy"),
        Command::Deps(_) => operation("deps"),
        Command::Secrets(_) => operation("secrets"),
        Command::Defer(_) => operation("defer"),
        Command::Exec(_) => operation("exec"),
        Command::State(_) => operation("state"),
        Command::Service(_) => operation("service"),
        Command::Rhai(_) => operation("rhai"),
        Command::Skill(_) => operation("skill:run"),
        Command::Docs(_) => operation("docs"),
        Command::Contracts(_) => operation("contracts"),
        Command::Bootstrap(_) => operation("bootstrap"),
        Command::Uninstall(_) => operation("uninstall"),
        Command::Release(_) => operation("release"),
        // Help is rendered by the guest binary itself, before dispatch.
        Command::Help(_) | Command::HelpGroup(_) => GuestOperation::Refused(
            "help is rendered from the guest binary and never crosses the channel".to_owned(),
        ),
        // The gateway owns a host listener and never runs in a guest (§8.6).
        Command::Gateway(_) => GuestOperation::Refused(
            "`effigy gateway` runs on the host, never in a container-mode guest; run it on the host \
             or ask the host session for the route table with `effigy container hosts`"
                .to_owned(),
        ),
        // The staged guest probe is the producer's own guest entry: the ordinary
        // dispatch runs it (guest_probe module), and it never crosses the
        // channel. A guest that asked for it is not making a host request.
        Command::InternalGuestProbe(_) => GuestOperation::Refused(
            "the staged guest probe is dispatched by the workspace producer, not by a guest command"
                .to_owned(),
        ),
        // Hidden internal entry points are refused at the guest CLI (§8.6).
        Command::InternalGateway(_)
        | Command::InternalGatewayIdentity(_)
        | Command::InternalScriptRun(_)
        | Command::InternalContainerLeaseReaper(_) => GuestOperation::Refused(
            "hidden internal entry points are refused in a container-mode guest".to_owned(),
        ),
    }
}

/// Why a guest never reached its host.
#[derive(Debug)]
pub(crate) enum ChannelError {
    /// The host refused before it recorded any effect for this request. The
    /// code is lane A's stable refusal code (§8.11).
    Refused { code: &'static str, message: String },
    /// The connection failed or ended before the host answered. Whether the
    /// host acted is `Unknown` and is never guessed; [`ChannelRequest::retry`]
    /// replays the same request id so the host answers from its record.
    Unknown { message: String, replayable: bool },
    /// The client would not form or send this request at all.
    Local(String),
}

impl ChannelError {
    fn from_refusal(refusal: channel::Refusal) -> Self {
        Self::Refused {
            code: refusal.code.as_str(),
            message: refusal.diagnostic,
        }
    }

    fn local(message: impl Into<String>) -> Self {
        Self::Local(message.into())
    }

    /// A transport failure: the request may or may not have been admitted, so
    /// the caller keeps its id and replays rather than admitting again.
    fn unknown(message: impl Into<String>) -> Self {
        Self::Unknown {
            message: message.into(),
            replayable: true,
        }
    }

    /// The host's sentence, with the refusal code so a script sees the same
    /// classification the host used.
    pub(crate) fn into_runner(self) -> RunnerError {
        match self {
            Self::Refused { code, message } => RunnerError::task_invocation(format!(
                "the workspace host refused this command ({code}): {message}"
            )),
            Self::Unknown { message, .. } => RunnerError::task_invocation(format!(
                "the workspace host did not answer, so whether it acted is unknown: {message}; \
                 retry the same command — the host answers a retry from its own record instead of \
                 admitting a second run"
            )),
            Self::Local(message) => RunnerError::task_invocation(message),
        }
    }

    fn is_replayable(&self) -> bool {
        matches!(
            self,
            Self::Unknown {
                replayable: true,
                ..
            }
        )
    }
}

/// What the host answered for one request.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChannelOutcome {
    /// A typed observation. It admitted nothing and recorded nothing.
    Observed(serde_json::Value),
    /// The host admitted this run for the request's key.
    Accepted { run: RunId, placement: RunPlacement },
    /// The run ended, with the host's own outcome and status.
    Completed {
        outcome: serde_json::Value,
        exit: ExitStatus,
    },
    /// The host opened this connection's attachment to a run it authorised.
    Attached(channel::AttachView),
}

/// The host's placement label, kept typed so a guest can never claim one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunPlacement {
    Host,
    Guest,
}

/// One request a guest intends to make: its id and body together, so a retry on
/// a fresh connection is the same intent to the host and never a second
/// admission (§8.11).
#[derive(Debug, Clone)]
pub(crate) struct Intent {
    id: RequestId,
    body: RequestBody,
}

impl Intent {
    pub(crate) fn new(body: RequestBody) -> Self {
        Self {
            id: RequestId::new(fresh_bytes()),
            body,
        }
    }

    /// One attach, at a caller's retained cursor. Replaying this same intent on
    /// a fresh connection names the same request id, so the host resumes the
    /// attachment it already recorded rather than opening a second one
    /// (§8.11). The cursor is where the caller's own stream reader stopped;
    /// `None` starts at the beginning.
    pub(crate) fn attach(
        run: RunId,
        streams: Vec<StreamKind>,
        cursors: std::collections::BTreeMap<u32, u64>,
    ) -> Self {
        Self::new(RequestBody::Attach(AttachBody {
            run,
            streams,
            cursor: None,
            cursors: (!cursors.is_empty()).then_some(cursors),
        }))
    }

    /// A fresh attach at the cursor this client has consumed so far. Use this
    /// after a `gap`, which is never presented as complete output: the caller
    /// asks again from where it truly stopped instead of replaying an intent
    /// whose answer the host already recorded.
    pub(crate) fn resume_attach(
        run: RunId,
        streams: Vec<StreamKind>,
        client: &WorkspaceClient<impl Transport>,
    ) -> Self {
        Self::attach(run, streams, client.resume_cursors())
    }

    pub(crate) fn id(&self) -> RequestId {
        self.id
    }
}

/// One channel connection, as far as one command needs it.
pub(crate) trait ChannelRequest {
    /// Authenticate this connection. The pinned peer was verified and the
    /// protocol negotiated before this is called, and the credential is read
    /// from guest storage only after that, so no capability crosses a
    /// connection whose peer is not the pinned host (§8.11).
    fn authenticate(&mut self, auth: GuestAuth) -> Result<(), ChannelError>;
    /// Send one intent. A transport failure returns [`ChannelError::Unknown`];
    /// the caller replays the same intent, whose id the host has already
    /// recorded or will record exactly once.
    fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError>;
    /// Acknowledge detach of a run this client admitted. A run this client did
    /// not admit is refused locally: no frame, so a secondary terminal can
    /// never stop the owner's run by leaving (§8.11).
    fn detach(&mut self, run: RunId) -> Result<(), ChannelError>;
}

/// Where a guest's capability comes from (§8.8, §8.11).
#[derive(Debug, Clone)]
pub(crate) enum GuestAuth {
    /// The capability the entry that opened this terminal handed over.
    Capability(Secret),
    /// The narrow credential Prepare installed, for a direct terminal.
    Enrolment {
        credential: Secret,
        session: SessionIdentity,
    },
}

/// The session a direct terminal enrols into: the run the context names and
/// the generation the host recorded for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionIdentity {
    pub(crate) run: RunId,
    pub(crate) generation: Generation,
}

/// Read the capability this guest may present, or the enrolment credential,
/// from guest-private storage. A missing, unreadable or mismatched credential
/// never unlocks anything: it reports host access unavailable and leaves local
/// reads working (§8.8).
///
/// The caller reaches this only after the pinned peer check, so a credential is
/// never read — let alone sent — for a peer that is not the pinned host.
pub(crate) fn load_auth(
    guest: &mut dyn GuestReader,
    context: &WorkspaceContextV2,
) -> Result<GuestAuth, RunnerError> {
    if let Some(bytes) = guest.read_file(&client_capability_path())? {
        let text = String::from_utf8_lossy(&bytes);
        let text = text.trim();
        if !text.is_empty() {
            let capability = Secret::parse(text).map_err(|_| {
                RunnerError::task_invocation(format!(
                    "the stored workspace capability in `{}` is not a capability; run \
                     `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
                    client_capability_path()
                ))
            })?;
            return Ok(GuestAuth::Capability(capability));
        }
    }
    let path = context.credential.path.as_str();
    let Some(bytes) = guest.read_file(path)? else {
        return Err(RunnerError::task_invocation(format!(
            "this terminal holds no workspace capability and the workspace installed no \
             enrolment credential at `{path}`; local reads work and host authority is unavailable. \
             Open the terminal with `effigy workspace`, or run the command on the host"
        )));
    };
    // The pinned document names the credential's own digest: bytes that differ
    // from what the host staged are refused instead of presented.
    let digest = context::sha256_text(&bytes);
    if digest != context.credential.digest {
        return Err(RunnerError::task_invocation(format!(
            "the workspace enrolment credential at `{path}` is {digest} but the context pins {}; \
             run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            context.credential.digest
        )));
    }
    let credential = parse_enrolment_credential(&String::from_utf8_lossy(&bytes))?;
    let session = enrolment_session(context)?;
    Ok(GuestAuth::Enrolment {
        credential,
        session,
    })
}

/// The session a direct terminal enrols into: the run and the generation the
/// strict v2 document publishes. Both are host-recorded facts, so the guest
/// presents exactly what it was given and never guesses one.
fn enrolment_session(context: &WorkspaceContextV2) -> Result<SessionIdentity, RunnerError> {
    let run = context.session.run_id.parse::<RunId>().map_err(|_| {
        RunnerError::task_invocation(format!(
            "the workspace context names session run `{}`, which is not a session identity; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace",
            context.session.run_id
        ))
    })?;
    if context.session.generation == 0 {
        return Err(RunnerError::task_invocation(
            "the workspace context publishes no admitted session generation, so this terminal \
             cannot enrol; run `effigy container up` on the host to prepare the workspace session"
                .to_owned(),
        ));
    }
    Ok(SessionIdentity {
        run,
        generation: Generation::new(context.session.generation),
    })
}

/// Parse the `key=value` body Prepare writes 0600 for the workspace user
/// (§8.8). The credential is never echoed or logged.
fn parse_enrolment_credential(body: &str) -> Result<Secret, RunnerError> {
    let field = |name: &str| {
        body.lines()
            .find_map(|line| line.trim().strip_prefix(name)?.strip_prefix('='))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let Some(credential) = field("credential") else {
        return Err(RunnerError::task_invocation(format!(
            "`{ENROL_CREDENTIAL_PATH}` carries no workspace credential; run \
             `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
        )));
    };
    Secret::parse(credential).map_err(|_| {
        RunnerError::task_invocation(format!(
            "`{ENROL_CREDENTIAL_PATH}` carries a credential that is not a workspace credential; \
             run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace"
        ))
    })
}

/// The version and digest this process runs as, so the context can be checked
/// against the executable that is actually running (§8.9, §8.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunningBinary {
    pub(crate) version: String,
    pub(crate) digest: String,
}

impl RunningBinary {
    pub(crate) fn observe() -> Result<Self, RunnerError> {
        let executable = std::env::current_exe().map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot identify the running workspace binary: {error}"
            ))
        })?;
        Ok(Self {
            version: effigy_core::build_info::display_version(),
            digest: digest_of_file(&executable)?,
        })
    }
}

/// `sha256:<hex>` of one file, streamed so a large binary is never held in
/// memory.
fn digest_of_file(path: &Path) -> Result<String, RunnerError> {
    use sha2::{Digest as _, Sha256};
    let mut file = std::fs::File::open(path).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot read the running workspace binary `{}`: {error}",
            path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot read the running workspace binary `{}`: {error}",
                path.display()
            ))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(context::render_digest(digest))
}

/// Build the run facts the strict context is verified against.
///
/// The binary half is this process's own (version and the digest of the
/// executable actually running); placement is the observation `detect` already
/// matched against the document. The environment generation and session are the
/// document's own, because the host binds both to its grant and fences every
/// request on its own records rather than on these claims (§8.11).
pub(crate) fn live_guest_run(
    published: &PublishedGuestContext,
    binary: RunningBinary,
) -> LiveGuestRun {
    LiveGuestRun {
        binary: context::GuestBinaryIdentity {
            version: binary.version,
            digest: binary.digest,
            protocol: context::CHANNEL_PROTOCOL_VERSION,
        },
        container: WorkspaceContainerIdentity {
            container_id: published.observation.container_id.clone(),
            vm_boot_id: published.observation.vm_boot_id.clone(),
            platform: published.observation.platform.clone(),
            libc: published.observation.libc.clone(),
        },
        environment_generation: published.document.environment_generation.to_string(),
        session_run_id: Some(published.document.session.run_id.clone()),
        session_generation: Some(published.document.session.generation),
        context_generation: published.generation,
    }
}

/// The identity `detect` already independently observed, re-presented so the
/// full context verification re-checks the same facts instead of re-reading
/// `/proc`.
pub(crate) struct ObservedGuest(GuestSelfObservation);

impl GuestIdentity for ObservedGuest {
    fn observe(&mut self) -> Result<GuestSelfObservation, identity::GuestIdentityRefusal> {
        Ok(self.0.clone())
    }
}

/// One authenticated channel connection: the guest client of §8.11 over the
/// transport an integration supplies.
pub(crate) struct WorkspaceClient<T: Transport> {
    transport: T,
    verified: VerifiedWorkspaceContext,
    live: LiveGuestRun,
    /// The one run this connection admitted. Only it may detach.
    admitted: Option<RunId>,
    /// The highest sequence this client consumed and acknowledged per stream,
    /// so a resume asks from where it stopped. A `gap` leaves a stream where it
    /// was: missing bytes are never reported as consumed.
    cursors: std::collections::BTreeMap<u32, u64>,
}

impl<T: Transport> fmt::Debug for WorkspaceClient<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WorkspaceClient")
            .field("context_generation", &self.verified.generation)
            .field("admitted", &self.admitted)
            .finish_non_exhaustive()
    }
}

impl<T: Transport> WorkspaceClient<T> {
    /// Pin, then negotiate, and hold the connection open for one command.
    ///
    /// The pinned host identity is checked before a single byte is written, so
    /// a mispinned peer receives no `hello` and — because no credential has
    /// been read yet — no credential bytes at all. Nothing else is read or
    /// written before the negotiation, and no request is sent before
    /// [`Self::authenticate`] succeeded (§8.11).
    pub(crate) fn connect(
        mut transport: T,
        verified: VerifiedWorkspaceContext,
        live: LiveGuestRun,
    ) -> Result<Self, ChannelError> {
        // §8.11 orders this first: the server identity the selected context
        // pinned, against the identity this transport verified. A peer that is
        // not the pinned host must learn nothing — not the schema, not the
        // version, not the digest, and certainly not a capability.
        let pinned = pinned_server_identity(&verified.channel)?;
        channel::check_server_pin(&pinned, transport.verified_peer().as_ref())
            .map_err(ChannelError::from_refusal)?;
        let session_run_id = parse_run_id(&verified.session_run_id)?;
        let hello = Hello {
            schema: channel::SCHEMA.to_owned(),
            protocol_versions: vec![channel::PROTOCOL_V1],
            effigy_version: live.binary.version.clone(),
            binary_digest: parse_digest(&live.binary.digest)?,
            context_generation: Generation::new(verified.generation),
            session_run_id,
        };
        channel::write_frame(&mut transport, &Frame::Hello(hello))
            .map_err(ChannelError::from_refusal)?;
        match read_frame(&mut transport)? {
            Frame::HelloOk(_) => {}
            Frame::Refuse(refusal) => return Err(ChannelError::from_refusal(refusal)),
            _ => {
                return Err(ChannelError::local(
                    "the workspace host did not answer the protocol negotiation",
                ))
            }
        }
        Ok(Self {
            transport,
            verified,
            live,
            admitted: None,
            cursors: std::collections::BTreeMap::new(),
        })
    }

    /// Present this connection's capability or enrolment credential.
    ///
    /// Authentication follows negotiation inside the connection. A failure
    /// arrives as the connection's refusal on the first response read, so no
    /// request can be built on an unauthenticated connection.
    pub(crate) fn authenticate(&mut self, auth: GuestAuth) -> Result<(), ChannelError> {
        let frame = match auth {
            GuestAuth::Capability(capability) => Frame::Authenticate(capability),
            GuestAuth::Enrolment {
                credential,
                session,
            } => Frame::Enrol(EnrolRequest {
                credential,
                capability: fresh_capability(),
                session_run_id: session.run,
                session_generation: session.generation,
                environment_generation: Generation::new(
                    self.live.environment_generation.parse().map_err(|_| {
                        ChannelError::local(
                            "the workspace context's environment generation is not a number",
                        )
                    })?,
                ),
                container_id: self.live.container.container_id.clone(),
                vm_boot_id: self.live.container.vm_boot_id.clone(),
            }),
        };
        channel::write_frame(&mut self.transport, &frame).map_err(ChannelError::from_refusal)
    }

    fn request_for(&self, intent: &Intent) -> Request {
        Request {
            id: intent.id(),
            context_generation: Generation::new(self.verified.generation),
            environment_generation: Generation::new(
                self.live.environment_generation.parse().unwrap_or_default(),
            ),
            session_run_id: parse_run_id(&self.verified.session_run_id)
                .expect("a verified context names a session run"),
            container_id: self.live.container.container_id.clone(),
            vm_boot_id: self.live.container.vm_boot_id.clone(),
            body: intent.body.clone(),
        }
    }

    /// Send one intent and read until the host answers it. A stream frame is
    /// acknowledged and dropped: output streams belong to the session front
    /// end, and this glue never attaches.
    fn exchange(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        let id = intent.id();
        let request = self.request_for(intent);
        channel::write_frame(&mut self.transport, &Frame::Request(request))
            .map_err(ChannelError::from_refusal)?;
        loop {
            let frame = read_frame(&mut self.transport)?;
            match frame {
                Frame::Response(response) if response.id == id => {
                    return match response.outcome {
                        channel::ResponseOutcome::Observed(value) => {
                            if matches!(intent.body, RequestBody::Attach(_)) {
                                // An attach answers with the host's own view of
                                // the attachment it opened; a body that is not
                                // one is not this client's business to guess.
                                let view: channel::AttachView = serde_json::from_value(value)
                                    .map_err(|_| {
                                        ChannelError::local(
                                            "the workspace host answered an attach with an \
                                             unreadable attachment view",
                                        )
                                    })?;
                                Ok(ChannelOutcome::Attached(view))
                            } else {
                                Ok(ChannelOutcome::Observed(value))
                            }
                        }
                        channel::ResponseOutcome::Accepted(accepted) => {
                            self.admitted = Some(accepted.run);
                            Ok(ChannelOutcome::Accepted {
                                run: accepted.run,
                                placement: match accepted.placement {
                                    effigy_kernel::ctx::Placement::Host => RunPlacement::Host,
                                    effigy_kernel::ctx::Placement::Guest => RunPlacement::Guest,
                                },
                            })
                        }
                        channel::ResponseOutcome::Completed(completed) => {
                            Ok(ChannelOutcome::Completed {
                                outcome: completed.outcome,
                                exit: completed.exit,
                            })
                        }
                        channel::ResponseOutcome::Refused(refusal) => {
                            Err(ChannelError::from_refusal(refusal))
                        }
                    };
                }
                Frame::Response(_) => {
                    return Err(ChannelError::local(
                        "the workspace host answered a request this connection did not send",
                    ))
                }
                // A connection-level refusal: no request of this connection
                // was dispatched, so nothing was admitted (§8.11).
                Frame::Refuse(refusal) => return Err(ChannelError::from_refusal(refusal)),
                Frame::Stream(stream) => {
                    // A gap names bytes this client never received, so it never
                    // advances the cursor; `end` reports the final sequence.
                    if stream.kind.is_data() || stream.kind == channel::StreamKind::End {
                        if stream.kind.is_data() {
                            channel::write_frame(
                                &mut self.transport,
                                &Frame::Stream(channel::StreamFrame {
                                    kind: channel::StreamKind::Ack,
                                    payload: None,
                                    rows: None,
                                    columns: None,
                                    ..stream.clone()
                                }),
                            )
                            .map_err(ChannelError::from_refusal)?;
                        }
                        let consumed = self.cursors.entry(stream.stream_id).or_insert(stream.seq);
                        *consumed = (*consumed).max(stream.seq);
                    }
                }
                _ => {
                    return Err(ChannelError::local(
                        "the workspace host sent a frame this connection did not expect",
                    ))
                }
            }
        }
    }
}

impl<T: Transport> ChannelRequest for WorkspaceClient<T> {
    fn authenticate(&mut self, auth: GuestAuth) -> Result<(), ChannelError> {
        WorkspaceClient::authenticate(self, auth)
    }

    fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        self.exchange(intent)
    }

    fn detach(&mut self, run: RunId) -> Result<(), ChannelError> {
        if self.admitted != Some(run) {
            return Err(ChannelError::Refused {
                code: channel::RefusalCode::Forbidden.as_str(),
                message: "this terminal did not admit that run, so it cannot detach it; a \
                          secondary terminal leaving never stops the owner's run"
                    .to_owned(),
            });
        }
        // The host answers an acknowledged detach with a typed observation.
        let intent = Intent::new(RequestBody::Detach(channel::RunBody { run }));
        self.exchange(&intent).map(|_| ())
    }
}

fn read_frame<T: Transport>(transport: &mut T) -> Result<Frame, ChannelError> {
    match channel::read_frame(transport) {
        Ok(Some(body)) => channel::decode_frame(&body).map_err(ChannelError::from_refusal),
        Ok(None) => Err(ChannelError::unknown(
            "the workspace host closed the connection before it answered",
        )),
        Err(refusal) => Err(match refusal.code {
            channel::RefusalCode::Unavailable => ChannelError::unknown(refusal.diagnostic),
            _ => ChannelError::from_refusal(refusal),
        }),
    }
}

impl<T: Transport> WorkspaceClient<T> {
    /// Where this connection's stream readers stopped, for a resume. A stream
    /// this client never read is absent, so the host resumes it from its
    /// retained frames rather than from a guess.
    pub(crate) fn resume_cursors(&self) -> std::collections::BTreeMap<u32, u64> {
        self.cursors.clone()
    }
}

/// The server identity the selected context pinned. A context without one has
/// no channel to pin, so nothing is sent.
fn pinned_server_identity(
    channel_locator: &context::WorkspaceChannelLocator,
) -> Result<Digest, ChannelError> {
    let pinned =
        channel_locator
            .server_identity
            .as_deref()
            .ok_or_else(|| ChannelError::Refused {
                code: channel::RefusalCode::Unavailable.as_str(),
                message: "the workspace context pins no host identity, so no connection is made"
                    .to_owned(),
            })?;
    parse_digest(pinned)
}

fn parse_run_id(text: &str) -> Result<RunId, ChannelError> {
    text.parse::<RunId>().map_err(|_| {
        ChannelError::local(format!(
            "`{text}` is not a workspace session run identity; run `{HOST_REPREPARE_COMMAND}` on \
             the host to re-prepare the workspace"
        ))
    })
}

/// `sha256:<hex>` as the wire's 32-byte digest.
fn parse_digest(text: &str) -> Result<Digest, ChannelError> {
    let hex = text.strip_prefix("sha256:").filter(|hex| hex.len() == 64);
    let Some(hex) = hex else {
        return Err(ChannelError::local(format!(
            "`{text}` is not a sha256 digest; run `{HOST_REPREPARE_COMMAND}` on the host to \
             re-prepare the workspace"
        )));
    };
    let mut bytes = [0u8; 32];
    for (index, pair) in hex.as_bytes().chunks(2).enumerate() {
        let nibble = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        let (Some(high), Some(low)) = (nibble(pair[0]), nibble(pair[1])) else {
            return Err(ChannelError::local(format!(
                "`{text}` is not a lowercase sha256 digest; run `{HOST_REPREPARE_COMMAND}` on the \
                 host to re-prepare the workspace"
            )));
        };
        bytes[index] = (high << 4) | low;
    }
    Ok(Digest(bytes))
}

/// A capability secret a direct terminal mints for itself when it enrols; the
/// host binds its scope, so the bytes grant nothing on their own (§8.11).
fn fresh_capability() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut bytes).is_ok() {
            return bytes;
        }
    }
    bytes[..16].copy_from_slice(&fresh_bytes());
    bytes
}

/// 128 bits of entropy for one request id, which the host records per session
/// generation (§8.11).
fn fresh_bytes() -> [u8; 16] {
    let mut bytes = [0u8; 16];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        if file.read_exact(&mut bytes).is_ok() {
            return bytes;
        }
    }
    // No usable entropy source: a random id is not authority, and the host
    // records whatever it is given, so a weak id cannot grant anything. It
    // only has to be unique per session generation, which the clock and
    // process id make likely; a collision is refused by the host as
    // `id_conflict` rather than admitted twice.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.subsec_nanos())
        .unwrap_or_default();
    bytes[..4].copy_from_slice(&std::process::id().to_be_bytes());
    bytes[4..8].copy_from_slice(&nanos.to_be_bytes());
    bytes
}

/// Open the one connection a guest command uses, through the pin check and the
/// protocol negotiation. It never receives a credential: the caller reads one
/// only after this returned a connection whose peer verified as the pinned host.
pub(crate) trait ChannelOpener {
    fn open(
        &mut self,
        verified: &VerifiedWorkspaceContext,
        live: &LiveGuestRun,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError>;
}

/// The production guest transport.
///
/// A context pins a locator only once a transport is qualified for its backend
/// (§8.3, §8.9, #504). The pinned TLS connection is that transport: it resolves
/// the rendered alias, dials the exact bound port and verifies the SPKI pin
/// during the handshake. This opener only selects it and refuses when the
/// context has no qualified locator or the transport cannot be opened; it never
/// reaches for host state the guest does not have.
#[derive(Debug)]
pub(crate) struct PinnedChannelTransport {
    pub(crate) backend: String,
    pub(crate) budget: std::time::Duration,
}

impl ChannelOpener for PinnedChannelTransport {
    fn open(
        &mut self,
        verified: &VerifiedWorkspaceContext,
        live: &LiveGuestRun,
    ) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        let unavailable = |message: String| ChannelError::Refused {
            code: channel::RefusalCode::Unavailable.as_str(),
            message,
        };
        if !verified.channel.is_ready() {
            return Err(unavailable(format!(
                "no host authority channel is qualified for backend `{}`; local reads work and \
                 host access is unavailable",
                self.backend
            )));
        }
        let mut opener = crate::runner::workspace_transport::opener::PinnedChannelOpener {
            timeout: self.budget,
        };
        let transport =
            crate::runner::workspace_transport::opener::ChannelOpener::open(&mut opener, verified)
                .map_err(|error| {
                    unavailable(format!(
                "the workspace host channel for backend `{}` could not be opened: {error}; local \
                 reads work and host access is unavailable",
                self.backend
            ))
                })?;
        let client =
            WorkspaceClient::connect(BoxedTransport(transport), verified.clone(), live.clone())?;
        Ok(Box::new(client))
    }
}

/// A boxed transport, so the opener's dynamic `Transport` can feed the client.
struct BoxedTransport(Box<dyn Transport>);

impl Transport for BoxedTransport {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.0.write_all(bytes)
    }
    fn verified_peer(&self) -> Option<channel::ServerIdentity> {
        self.0.verified_peer()
    }
}

/// Run one command inside a prepared container-mode workspace.
///
/// A local read is answered from the pinned context. Everything else crosses
/// the one channel; a refusal is returned as an error and never retried
/// against guest state or a guest engine.
pub(crate) fn dispatch(
    cmd: &Command,
    published: &PublishedGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
    opener: &mut dyn ChannelOpener,
) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(|error| {
        RunnerError::task_invocation(format!("cannot read the working directory: {error}"))
    })?;
    dispatch_in(cmd, published, guest, checkout, opener, &cwd)
}

/// [`dispatch`] from a named working directory, so a proof does not depend on
/// where the test process happens to stand.
pub(crate) fn dispatch_in(
    cmd: &Command,
    published: &PublishedGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
    opener: &mut dyn ChannelOpener,
    cwd: &Path,
) -> Result<String, RunnerError> {
    let wants_json = effigy_cli::command_requests_json(cmd, false);
    match classify(cmd) {
        GuestOperation::Local(read) => run_local(read, published, guest, checkout, wants_json),
        GuestOperation::Refused(message) => Err(RunnerError::task_invocation(message)),
        GuestOperation::Observe { op, target } => {
            let body = RequestBody::Observe(ObserveBody {
                op,
                target: Some(target),
            });
            ask_host(published, guest, opener, body, wants_json, cwd)
        }
        GuestOperation::Invoke { selector, target } => {
            let body = RequestBody::Invoke(InvocationBody {
                selector,
                cwd: checkout_relative(&published.document.paths, cwd)?,
                target,
                args: invocation_args(cmd)?,
                output: if wants_json {
                    ChannelOutputMode::Json
                } else {
                    ChannelOutputMode::Text
                },
            });
            ask_host(published, guest, opener, body, wants_json, cwd)
        }
    }
}

/// Serve one local read from the strict pinned document and the mounted
/// checkout. Nothing here admits, enrols or opens a connection.
fn run_local(
    read: LocalRead,
    published: &PublishedGuestContext,
    guest: &mut dyn GuestReader,
    checkout: &mut dyn CheckoutReader,
    wants_json: bool,
) -> Result<String, RunnerError> {
    let mut reads = GuestLocalReads {
        context: &published.document,
        guest,
        checkout,
    };
    let outcome = reads
        .read(&read, published.generation)
        .map_err(local_read_error)?;
    Ok(render_local(outcome, wants_json))
}

/// Ask the host, once. A connection that dies before answering is replayed on
/// one fresh connection under the same request id, so the host answers from its
/// record rather than admitting a second run (§8.11).
///
/// Order is the contract's: verify the strict context against this guest's own
/// observation, open the pinned connection and let it check the server pin and
/// negotiate, and only then read a credential from guest storage and present
/// it. A mispinned peer therefore learns nothing and never causes a credential
/// to be read, let alone sent.
fn ask_host(
    published: &PublishedGuestContext,
    guest: &mut dyn GuestReader,
    opener: &mut dyn ChannelOpener,
    body: RequestBody,
    wants_json: bool,
    cwd: &Path,
) -> Result<String, RunnerError> {
    let class = match &body {
        RequestBody::Observe(_) => GuestOperationClass::Observe,
        _ => GuestOperationClass::Mutate,
    };
    let live = live_guest_run(published, RunningBinary::observe()?);
    let verified = published
        .document
        .verify(&mut ObservedGuest(published.observation.clone()), &live)
        .map_err(context_refusal_error)?;
    let mut client = opener
        .open(&verified, &live)
        .map_err(ChannelError::into_runner)?;
    let auth = load_auth(guest, &published.document)?;
    // Only an authority-bearing request enrols a direct terminal; a Static or
    // Observe request that holds no capability refuses instead (§8.8).
    let auth = match (auth, class.requires_enrolment()) {
        (GuestAuth::Capability(capability), _) => GuestAuth::Capability(capability),
        (auth @ GuestAuth::Enrolment { .. }, true) => auth,
        (GuestAuth::Enrolment { .. }, false) => {
            return Err(RunnerError::task_invocation(format!(
                "this command reads live host facts, and this terminal holds only the enrolment \
                 credential, which authorises enrolment for an authority-bearing command and \
                 never an observation; open the terminal with `effigy workspace` or run the \
                 command on the host (started in `{}`)",
                cwd.display()
            )))
        }
    };
    let intent = Intent::new(body);
    let outcome = match client
        .authenticate(auth.clone())
        .and_then(|()| client.send(&intent))
    {
        Ok(outcome) => Ok(outcome),
        Err(error) if error.is_replayable() => {
            let mut replay = opener
                .open(&verified, &live)
                .map_err(ChannelError::into_runner)?;
            replay
                .authenticate(auth)
                .and_then(|()| replay.send(&intent))
        }
        Err(error) => Err(error),
    };
    outcome
        .map(|outcome| render_outcome(outcome, wants_json))
        .map_err(ChannelError::into_runner)
}

/// The guest's own argv tail, as the channel carries it. A task's own
/// arguments come from its parsed invocation; every other command carries the
/// arguments the operator typed, which the host re-parses against its own
/// records (§8.11).
fn invocation_args(cmd: &Command) -> Result<Vec<String>, RunnerError> {
    if let Command::Task(task) = cmd {
        return Ok(task.args.clone());
    }
    let mut args = Vec::new();
    for argument in std::env::args_os().skip(1) {
        let argument = argument.into_string().map_err(|_| {
            RunnerError::task_invocation(
                "a container-mode guest sends its arguments over the workspace channel, which \
                 carries UTF-8 text; rename the argument to cross the channel",
            )
        })?;
        args.push(argument);
    }
    Ok(args)
}

fn checkout_relative(paths: &WorkspacePathMap, cwd: &Path) -> Result<String, RunnerError> {
    let root = Path::new(paths.guest_checkout.trim_end_matches('/'));
    let relative = cwd.strip_prefix(root).map_err(|_| {
        RunnerError::task_invocation(format!(
            "`effigy` must run inside the workspace checkout `{}` to use the host channel; this \
             command started in `{}`",
            paths.guest_checkout,
            cwd.display()
        ))
    })?;
    let rendered = relative.to_string_lossy().replace('\\', "/");
    if rendered.is_empty() {
        return Ok(".".to_owned());
    }
    if rendered.starts_with('/') || rendered.split('/').any(|part| part == "..") {
        return Err(RunnerError::task_invocation(format!(
            "`{}` is not inside the workspace checkout `{}`",
            cwd.display(),
            paths.guest_checkout
        )));
    }
    Ok(rendered)
}

fn render_local(outcome: LocalReadOutcome, wants_json: bool) -> String {
    match outcome {
        LocalReadOutcome::Version {
            host_version,
            guest_version,
            digest,
            protocol,
            context_generation,
        } => {
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "version": guest_version,
                    "host_version": host_version,
                    "digest": digest,
                    "protocol": protocol,
                    "context_generation": context_generation,
                    "placement": "guest",
                })
                .to_string()
            } else {
                format!("{guest_version} (host {host_version}, {digest}, protocol {protocol})")
            }
        }
        LocalReadOutcome::Tasks(tasks) => {
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "tasks": tasks,
                })
                .to_string()
            } else {
                tasks.join("\n")
            }
        }
        LocalReadOutcome::Sources(sources) => {
            let rendered = sources
                .into_iter()
                .map(|source| format!("{} {} {}", source.kind, source.name, source.digest))
                .collect::<Vec<_>>();
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "sources": rendered,
                })
                .to_string()
            } else {
                rendered.join("\n")
            }
        }
        LocalReadOutcome::CheckoutFile(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
    }
}

fn render_outcome(outcome: ChannelOutcome, wants_json: bool) -> String {
    match outcome {
        ChannelOutcome::Observed(value) => render_value(value, wants_json),
        // The run is admitted on the host; its output and settlement belong to
        // the session front end that attaches to it (§8.7).
        ChannelOutcome::Accepted { run, placement } => {
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "run": run.to_string(),
                    "placement": match placement {
                        RunPlacement::Host => "host",
                        RunPlacement::Guest => "guest",
                    },
                })
                .to_string()
            } else {
                format!("admitted run {run} on the workspace host")
            }
        }
        ChannelOutcome::Attached(view) => {
            if wants_json {
                serde_json::to_string(&view).unwrap_or_else(|_| "{}".to_owned())
            } else {
                format!("attached to run {} from cursor {}", view.run, view.cursor)
            }
        }
        ChannelOutcome::Completed { outcome, exit } => {
            let rendered = render_value(outcome, wants_json);
            let status = match (exit.code, exit.signal) {
                (Some(code), _) => format!("exit {code}"),
                (None, Some(signal)) => format!("signal {signal}"),
                (None, None) => "status not recorded".to_owned(),
            };
            if wants_json {
                serde_json::json!({
                    "schema": "effigy.workspace-client.v1",
                    "outcome": rendered,
                    "status": status,
                })
                .to_string()
            } else {
                format!("{rendered} ({status})")
            }
        }
    }
}

fn render_value(value: serde_json::Value, wants_json: bool) -> String {
    if wants_json {
        return value.to_string();
    }
    match value {
        serde_json::Value::String(text) => text,
        other => serde_json::to_string_pretty(&other).unwrap_or_else(|_| other.to_string()),
    }
}

fn context_read_error(error: ContextReadError) -> RunnerError {
    match error {
        ContextReadError::Guest(error) => error,
        ContextReadError::Refused(refusal) => context_refusal_error(refusal),
    }
}

fn context_refusal_error(refusal: ContextRefusal) -> RunnerError {
    let next = if refusal.names_reprepare() {
        format!(" Run `{HOST_REPREPARE_COMMAND}` on the host to re-prepare the workspace.")
    } else {
        String::new()
    };
    RunnerError::task_invocation(format!(
        "the workspace context refused this command ({}): {}{next}",
        refusal.code.as_str(),
        refusal.message
    ))
}

fn local_read_error(error: LocalReadError) -> RunnerError {
    match error {
        LocalReadError::Guest(error) => error,
        LocalReadError::Refused(refusal) => context_refusal_error(refusal),
        LocalReadError::HostAuthorityRequired(operation) => RunnerError::task_invocation(
            LocalReadError::HostAuthorityRequired(operation).to_string(),
        ),
    }
}

/// The dispatch entry the runner calls before its host dispatch.
///
/// `Ok(None)` means this process is not a prepared container-mode guest: the
/// ordinary host dispatch runs unchanged, and nothing was written. Hybrid mode
/// always takes this path (§8.10 R1).
pub(crate) fn guest_command(cmd: &Command) -> Result<Option<String>, RunnerError> {
    // The staged guest probe is executed by the producer inside the created
    // container. It is the guest's own entry, handled by `guest_probe`, so it
    // takes the ordinary dispatch rather than the channel.
    if matches!(cmd, Command::InternalGuestProbe(_)) {
        return Ok(None);
    }
    let mut guest = ContainerGuest;
    let published = match detect(&mut guest)? {
        GuestPlacement::Host => return Ok(None),
        GuestPlacement::Guest(published) => published,
    };
    let mut checkout = MountedCheckout::new(&published.document.paths);
    let mut opener = PinnedChannelTransport {
        backend: published.document.backend.clone(),
        budget: CHANNEL_OPEN_BUDGET,
    };
    dispatch(cmd, &published, &mut guest, &mut checkout, &mut opener).map(Some)
}

/// A container-mode guest never runs the host engines locally, so a raw
/// stdio task passthrough is refused here rather than in the host dispatch.
pub(crate) fn guest_passthrough_refusal() -> Result<(), RunnerError> {
    let mut guest = ContainerGuest;
    match detect(&mut guest)? {
        GuestPlacement::Host => Ok(()),
        GuestPlacement::Guest(_) => Err(RunnerError::task_invocation(format!(
            "`effigy skill run --stdio passthrough` runs a task, and a container-mode guest has no \
             host authority to admit or run one. Run it on the host, or open the terminal with \
             `effigy workspace`; `{HOST_REPREPARE_COMMAND}` on the host re-prepares the workspace"
        ))),
    }
}

#[cfg(test)]
#[path = "workspace_client/tests.rs"]
mod tests;
