//! The Level 1 non-interactive forwarding seam (contract 052 §8.11).
//!
//! Tom's 2026-11-01 ruling makes the v0.15 workspace client non-interactive. A
//! forwarded command:
//!
//! - runs on the host with no PTY and with stdin already at EOF. The host run
//!   that carries it is the reserved managed run `HostDispatch::admit` starts
//!   (`Front::Detached`, `terminal: None`), whose captured stdin read end is
//!   dropped at creation, so the child reads EOF and never sees a terminal
//!   (`effigy_kernel::custody::spawn`).
//! - keeps one authenticated connection open from admission to the exact run's
//!   terminal outcome. The connection takes the run's *owning* attachment while
//!   the run is live, so this connection's end cancels the run through kernel
//!   custody; it never takes a second one.
//! - delivers stdout and stderr separately and incrementally to the guest's own
//!   sinks. Bytes are only acknowledged to the host after the sink accepted
//!   them, so a partial write or a dead sink never consumes output the guest
//!   did not print.
//! - reports the run's recorded terminal status, and reports an *unknown* result
//!   — never success — when the outcome, a byte range or the connection cannot
//!   be established.
//!
//! ## Why output is read through an observe, not a stream frame
//!
//! The channel's `StreamFrame` machinery is the terminal surface: it publishes
//! an attached run's output while its attachment row is live, and both its
//! acknowledgement path and `RetainedStreams` refuse once the run closes
//! (`Attachment::attached_observed` → `require_live_owner`). A forwarded command
//! is exactly the case where the run closes as soon as it ends, so a tail
//! published through that machinery can be lost between the last pump and the
//! close, and the losing case is indistinguishable from a quiet run — it would
//! report success with missing bytes.
//!
//! The kernel's captured log is not attachment-scoped (`run::read_log` reads it
//! by run and absolute offset). This module therefore reads it through the
//! session's own `Observe` surface — a read that admits nothing and writes
//! nothing — with the guest's consumed position carried in the operation name.
//! Each response is bounded, the guest asks for the next chunk only after it
//! delivered the previous one, and the drain after the terminal outcome still
//! sees the run's final retained bytes. A gap, a truncated capture or an
//! unavailable read is a refusal, so rotated or missing output is never
//! presented as complete.
//!
//! The guest never sends terminal input, resize or a detach. The guest command
//! filter refuses a terminal-shaped command before it opens a connection, this
//! module's host `Streams` refuses terminal input and resize with the named
//! Level 2 message, and the Level 1 client's own connection refuses an
//! acknowledged detach by name before any frame. (The kernel serves a `detach`
//! request itself and exposes no runner seam to refuse it on the host; the
//! guest is the side that must never form one, and Level 2 owns the host
//! refusal when it adds acknowledged detach.)
//!
//! ## Level 2 boundary
//!
//! Guest attach/reconnect, a PTY or any interactive terminal, terminal
//! input/resize delivery and an acknowledged detach are Level 2 (052 §8.11) and
//! are refused by name here. Coverage of the wider command surface is the
//! Level 1 command policy owned by the `512-l1-policy` work; this module exposes
//! the refusal text that policy should reuse rather than duplicating it.

use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;

use base64::Engine as _;
use effigy_cli::{Command, ContainerSubcommand};
use effigy_kernel::ctx::ObserveContext;
use effigy_kernel::id::RunId;
use effigy_kernel::observe::Observed;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{self, Attachment};
use effigy_kernel::workspace_channel::{
    self as channel, Cursors, ExitStatus, Refusal, StreamChunk, StreamKind, StreamStart, Streams,
    TerminalSize,
};
use serde::{Deserialize, Serialize};

use super::context::v2::PublishedGuestContext;
use super::context::{GuestReader, LiveGuestRun, VerifiedWorkspaceContext};
use super::terminal::TerminalCustody;
use super::{
    ChannelError, ChannelOpener, ChannelOutcome, ChannelRequest, Intent, RunPlacement, Selector,
};
use crate::runner::error::RunnerError;

/// The named Level 2 refusal for a request that needs an interactive terminal.
pub(crate) const LEVEL_TWO_INTERACTIVE: &str =
    "workspace Level 2 unsupported: an interactive terminal is not part of the v0.15 workspace \
     client; run the command on the host";

/// The named Level 2 refusal for guest attach and reconnect.
pub(crate) const LEVEL_TWO_ATTACH: &str =
    "workspace Level 2 unsupported: guest attach and reconnect are not part of the v0.15 \
     workspace client; run `effigy workspace` on the host";

/// The named Level 2 refusal for terminal input.
pub(crate) const LEVEL_TWO_INPUT: &str =
    "workspace Level 2 unsupported: terminal input is not part of the v0.15 workspace client; \
     forwarded commands are non-interactive and read no stdin";

/// The named Level 2 refusal for a terminal resize.
pub(crate) const LEVEL_TWO_RESIZE: &str =
    "workspace Level 2 unsupported: terminal resize is not part of the v0.15 workspace client; \
     forwarded commands have no terminal";

/// The named Level 2 refusal for an acknowledged detach.
pub(crate) const LEVEL_TWO_DETACH: &str =
    "workspace Level 2 unsupported: an acknowledged detach is not part of the v0.15 workspace \
     client; the owning connection ends with its forwarded command";

/// One interactive request the v0.15 client does not offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InteractiveRequest {
    Attach,
    Pty,
    Input,
    Resize,
    Detach,
}

impl InteractiveRequest {
    /// The stable Level 2 sentence a refusal carries.
    pub(crate) fn refusal(self) -> &'static str {
        match self {
            Self::Attach => LEVEL_TWO_ATTACH,
            Self::Pty => LEVEL_TWO_INTERACTIVE,
            Self::Input => LEVEL_TWO_INPUT,
            Self::Resize => LEVEL_TWO_RESIZE,
            Self::Detach => LEVEL_TWO_DETACH,
        }
    }
}

/// The refusal for a guest command that needs an interactive terminal, decided
/// before any connection is opened or any run is admitted.
///
/// This is deliberately narrow: the whole command surface, including the
/// mutating operations a guest must not reach, belongs to the Level 1 command
/// policy (`512-l1-policy`). What belongs here is the part whose only reason to
/// exist is a terminal the Level 1 client does not have.
pub(crate) fn interactive_command_refusal(cmd: &Command) -> Option<&'static str> {
    match cmd {
        // The workspace entry attaches a terminal to the live session run.
        Command::Workspace(_) => Some(InteractiveRequest::Attach.refusal()),
        Command::Container(args) => match args.subcommand {
            // `container shell` hands the container a terminal. There is no
            // terminal to hand it in Level 1.
            ContainerSubcommand::Shell { .. } => Some(InteractiveRequest::Pty.refusal()),
            _ => None,
        },
        _ => None,
    }
}

/// The prefix of the session's captured-output read. The operation name carries
/// the run and this client's consumed position because an observe request has no
/// other field: `runs:output:<run>:<stdout-offset>:<stderr-offset>`.
pub(crate) const OUTPUT_OP_PREFIX: &str = "runs:output";

/// The most bytes one observe returns per stream. Two streams therefore stay
/// far below the channel's frame bound, and the guest never buffers more than
/// one bounded chunk per stream before it writes them.
const OUTPUT_CHUNK: usize = 16 * 1024;

/// How long the guest waits between two output reads while its run is live.
/// Output is delivered within this interval of being captured, and a live
/// command costs twenty bounded reads a second.
const OUTPUT_POLL: Duration = Duration::from_millis(50);

/// How many output reads pass between two terminal-outcome checks. The outcome
/// is a replay of the same intent, which the host answers from its own record;
/// the interval bounds how long the client can wait to notice a closed run.
const OUTCOME_CHECK_EVERY: u32 = 5;

/// The schema of one captured-output read.
const OUTPUT_SCHEMA: &str = "effigy.workspace-output.v1";

/// The operation name that reads `run`'s captured output from `cursors`.
pub(crate) fn output_operation(run: RunId, cursors: [u64; 2]) -> String {
    format!("{OUTPUT_OP_PREFIX}:{run}:{}:{}", cursors[0], cursors[1])
}

/// The run and consumed position an output operation names, or `None` when the
/// operation is not one.
fn parse_output_operation(op: &str) -> Option<(RunId, [u64; 2])> {
    let rest = op.strip_prefix(OUTPUT_OP_PREFIX)?.strip_prefix(':')?;
    let mut parts = rest.split(':');
    let run = parts.next()?.parse::<RunId>().ok()?;
    let stdout = parts.next()?.parse::<u64>().ok()?;
    let stderr = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((run, [stdout, stderr]))
}

/// The retained range the log no longer holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GapRange {
    pub(crate) from: u64,
    pub(crate) to: u64,
}

/// One bounded read of one captured stream, as the observe reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StreamRead {
    /// The offset the client asked from.
    pub(crate) from: u64,
    /// The offset of the first byte the client has not consumed yet.
    pub(crate) next: u64,
    /// Base64 of at most [`OUTPUT_CHUNK`] captured bytes.
    pub(crate) bytes: String,
    /// The range the retained log no longer holds, when the client's position
    /// was rotated away. A gap is never presented as complete output.
    pub(crate) gap: Option<GapRange>,
    /// The capture itself is truncated, so no read can be complete.
    pub(crate) truncated: bool,
}

impl StreamRead {
    /// Whether this read delivered nothing.
    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Decode the delivered bytes.
    fn decode(&self) -> Result<Vec<u8>, Refusal> {
        base64::engine::general_purpose::STANDARD
            .decode(self.bytes.as_bytes())
            .map_err(|_| Refusal::invalid("a captured-output read is not valid base64"))
    }
}

/// One captured-output read: both streams, in the guest's own order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OutputRead {
    pub(crate) schema: String,
    pub(crate) run: RunId,
    pub(crate) stdout: StreamRead,
    pub(crate) stderr: StreamRead,
}

/// The placement label a person reads, never a Rust variant name.
fn placement_label(placement: RunPlacement) -> &'static str {
    match placement {
        RunPlacement::Host => "host",
        RunPlacement::Guest => "guest",
    }
}

/// What kind of answer the host sent, in words rather than a variant name.
fn outcome_label(outcome: &ChannelOutcome) -> &'static str {
    match outcome {
        ChannelOutcome::Observed(_) => "a read",
        ChannelOutcome::Accepted { .. } => "an admission",
        ChannelOutcome::Completed { .. } => "a terminal outcome",
        ChannelOutcome::Attached(_) => "an attachment",
    }
}

/// Read one captured stream from `offset`.
fn read_stream(
    observe: &ObserveContext,
    root: &StateRoot,
    run: RunId,
    stream: effigy_kernel::custody::Stream,
    offset: u64,
) -> Result<StreamRead, Refusal> {
    let empty = |offset: u64| StreamRead {
        from: offset,
        next: offset,
        bytes: String::new(),
        gap: None,
        truncated: false,
    };
    match run::read_log(
        observe,
        root,
        run,
        effigy_kernel::supervise::OUTPUT_SEQUENCE,
        stream,
        offset,
        OUTPUT_CHUNK,
    ) {
        // A stream that was never captured is empty, not missing: a command
        // that wrote nothing to stderr is an ordinary success.
        Observed::Absent(_) if offset == 0 => Ok(empty(0)),
        // A consumed position whose log is gone cannot be continued, and the
        // read cannot tell rotation from removal. Refuse instead of reporting
        // the stream as finished.
        Observed::Absent(_) => Err(Refusal::forbidden(format!(
            "the retained output for this run is no longer available at offset {offset}; the \
             forwarded command's output is incomplete"
        ))),
        Observed::Unknown(reason) => Err(Refusal::unknown(format!(
            "the run's captured output could not be read: {reason}"
        ))),
        Observed::Verified(read, _) => Ok(StreamRead {
            from: read.from,
            next: read.from.saturating_add(read.bytes.len() as u64),
            bytes: base64::engine::general_purpose::STANDARD.encode(&read.bytes),
            // `from` past the client's position is a retained range the log no
            // longer holds. It is reported as a gap and never counted as read.
            gap: (read.from > offset).then_some(GapRange {
                from: offset,
                to: read.from,
            }),
            truncated: read.truncated,
        }),
    }
}

/// Serve one captured-output read.
///
/// This is an observe: it opens no run, takes no gate, admits nothing and
/// writes nothing. `root` scopes the read to this session's environment
/// journal, like the merged `runs:list` and `runs:status` reads; the grant the
/// request presented binds the session, environment and container.
pub(crate) fn observe_output(
    observe: &ObserveContext,
    root: &StateRoot,
    op: &str,
) -> Result<serde_json::Value, Refusal> {
    let Some((run, [stdout, stderr])) = parse_output_operation(op) else {
        return Err(Refusal::invalid(
            "a captured-output read names a run and the consumed offsets",
        ));
    };
    let read = OutputRead {
        schema: OUTPUT_SCHEMA.to_owned(),
        run,
        stdout: read_stream(
            observe,
            root,
            run,
            effigy_kernel::custody::Stream::Stdout,
            stdout,
        )?,
        stderr: read_stream(
            observe,
            root,
            run,
            effigy_kernel::custody::Stream::Stderr,
            stderr,
        )?,
    };
    serde_json::to_value(read)
        .map_err(|_| Refusal::unavailable("a captured-output read is not encodable"))
}

/// Where forwarded bytes go on the guest.
///
/// A sink accepts the whole slice or reports the failure: a partial write is
/// never acknowledged, so the host's retained position never moves past bytes
/// the guest did not print.
pub(crate) trait CommandSinks {
    fn stdout(&mut self, bytes: &[u8]) -> std::io::Result<()>;
    fn stderr(&mut self, bytes: &[u8]) -> std::io::Result<()>;
}

/// The guest's own standard streams.
///
/// A forwarded command owns its stdout in text mode. In JSON mode the returned
/// envelope owns stdout, so captured stdout is consumed (its position still
/// advances) but not printed; stderr is always the command's.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcessSinks {
    pub(crate) stdout: bool,
}

impl ProcessSinks {
    /// The sinks for one forwarded command.
    pub(crate) fn new(stdout: bool) -> Self {
        Self { stdout }
    }
}

impl CommandSinks for ProcessSinks {
    fn stdout(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if !self.stdout {
            return Ok(());
        }
        let mut out = std::io::stdout().lock();
        out.write_all(bytes)?;
        out.flush()
    }

    fn stderr(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let mut err = std::io::stderr().lock();
        err.write_all(bytes)?;
        err.flush()
    }
}

/// The terminal status a replayed admission recorded, or `None` when it cannot
/// be established.
///
/// The kernel's `Completed` response carries the host's `RecordedOutcome`; its
/// `exit` field is `UNKNOWN` for a replayed admission today, so the recorded
/// workload outcome is the host's own statement of the status. Nothing here is
/// inferred from the guest's own view:
///
/// - `Succeeded` is exit 0,
/// - `Failed { code }` is that code,
/// - `Cancelled` is its signal, or the signalling stop cause.
///
/// Anything else — `Unknown`, `Interrupted`, `TimedOut`, `NotStarted`, or an
/// unreadable document — has no status, and the caller reports unknown.
pub(crate) fn recorded_status(exit: ExitStatus, outcome: &serde_json::Value) -> Option<ExitStatus> {
    if exit.code.is_some() || exit.signal.is_some() {
        return Some(exit);
    }
    let workload = outcome.get("workload")?;
    if workload.as_str() == Some("Succeeded") {
        return Some(ExitStatus {
            code: Some(0),
            signal: None,
        });
    }
    if let Some(code) = workload
        .get("Failed")
        .and_then(|failed| failed.get("code"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|code| i32::try_from(code).ok())
    {
        return Some(ExitStatus {
            code: Some(code),
            signal: None,
        });
    }
    if let Some(cancelled) = workload.get("Cancelled") {
        let signal = cancelled
            .get("signal")
            .and_then(serde_json::Value::as_i64)
            .or_else(|| {
                cancelled
                    .get("cause")
                    .and_then(|cause| cause.get("Signal"))
                    .and_then(serde_json::Value::as_i64)
            })
            .and_then(|signal| i32::try_from(signal).ok());
        // A cancellation without a signal is a stop request, not a terminal
        // status the guest can name; it is reported as unknown.
        return signal.map(|signal| ExitStatus {
            code: None,
            signal: Some(signal),
        });
    }
    None
}

/// What one forwarded command has delivered so far.
#[derive(Debug, Default)]
struct Delivered {
    /// The first captured byte this guest has not consumed on stdout and
    /// stderr.
    cursors: [u64; 2],
    /// Output reads asked for so far, so the outcome check runs on its own
    /// cadence instead of on every read.
    turns: u32,
    /// A gap or a truncated capture was seen, so the output is incomplete and
    /// the command cannot report success.
    incomplete: bool,
}

/// Opens one guest connection, and can replace it when the current one must be
/// replayed.
///
/// A production forwarder opens a pinned, negotiated connection from the
/// verified context; a proof supplies scripted connections through the same
/// seam, so the replay state machine below is the one that runs in both.
pub(crate) trait Connections {
    fn open(&mut self) -> Result<Box<dyn ChannelRequest>, ChannelError>;
}

/// The production connections: the guest's pinned opener and the facts it
/// verified.
struct GuestConnections<'a> {
    opener: &'a mut dyn ChannelOpener,
    verified: VerifiedWorkspaceContext,
    live: LiveGuestRun,
}

impl Connections for GuestConnections<'_> {
    fn open(&mut self) -> Result<Box<dyn ChannelRequest>, ChannelError> {
        self.opener.open(&self.verified, &self.live)
    }
}

/// The Level 1 client's connection: it never acknowledges a detach.
///
/// A Level 1 connection ends with its forwarded command, so there is no
/// acknowledged detach to send. Refusing here is the named Level 2 answer and it
/// happens before a frame, so no host effect can follow. (The kernel serves a
/// `detach` request itself and has no runner seam to refuse it on the host; the
/// guest is the side that must not form one.)
struct LevelOneChannel(Box<dyn ChannelRequest>);

impl ChannelRequest for LevelOneChannel {
    fn authenticate(&mut self, auth: super::GuestAuth) -> Result<(), ChannelError> {
        self.0.authenticate(auth)
    }

    fn send(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        self.0.send(intent)
    }

    fn detach(&mut self, _run: RunId) -> Result<(), ChannelError> {
        Err(ChannelError::Local(
            InteractiveRequest::Detach.refusal().to_owned(),
        ))
    }
}

/// The Level 1 forwarder's authenticated connection, with the replay that
/// recovers an indeterminate send on a fresh connection.
///
/// The credential is presented lazily by the first request, exactly as
/// `ask_host` orders it: negotiation is already done, so a mispinned peer has
/// learned nothing and the credential is read only after the pin check. An
/// indeterminate delivery opens a fresh connection, presents the same
/// credential once there, and adopts it — and the session records that the
/// replacement presented it, so a later request never presents it twice. The
/// two operations are asked for per use, never borrowed across a replay.
pub(crate) trait ForwardChannel {
    /// The live connection.
    fn client(&mut self) -> &mut dyn ChannelRequest;
    /// Send the same intent, replaying on a fresh authenticated connection when
    /// this one's delivery became indeterminate.
    fn replay(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError>;
}

/// One forwarder's authenticated connection and the state that keeps it honest.
pub(crate) struct Session<'a> {
    connections: Box<dyn Connections + 'a>,
    /// The credential, read only after the first connection's pin check and
    /// negotiation. The first request presents it.
    auth: Option<super::GuestAuth>,
    client: LevelOneChannel,
    /// The credential was presented on the live connection. A replacement
    /// presents it itself, so this is set for the replacement too.
    presented: bool,
}

impl<'a> Session<'a> {
    /// Open the first connection: the pinned peer check and the protocol
    /// negotiation happen here, before any credential is read.
    pub(crate) fn connect(
        mut connections: Box<dyn Connections + 'a>,
    ) -> Result<Self, ChannelError> {
        let client = connections.open()?;
        Ok(Self {
            connections,
            auth: None,
            client: LevelOneChannel(client),
            presented: false,
        })
    }

    /// Hand over the credential the guest read after the pin check. Nothing is
    /// presented until the first request.
    pub(crate) fn authenticate_with(&mut self, auth: super::GuestAuth) {
        self.auth = Some(auth);
    }

    fn present(&mut self) -> Result<(), ChannelError> {
        if self.presented {
            return Ok(());
        }
        let auth = self.auth.clone().ok_or_else(|| {
            ChannelError::Local(
                "the workspace client has no credential to present after negotiation".to_owned(),
            )
        })?;
        self.client.authenticate(auth)?;
        self.presented = true;
        Ok(())
    }
}

impl ForwardChannel for Session<'_> {
    fn client(&mut self) -> &mut dyn ChannelRequest {
        debug_assert!(
            self.presented,
            "a request needs the authenticated connection the first intent opened"
        );
        &mut self.client
    }

    fn replay(&mut self, intent: &Intent) -> Result<ChannelOutcome, ChannelError> {
        let sent = self.present().and_then(|()| self.client.send(intent));
        match sent {
            Ok(outcome) => Ok(outcome),
            Err(error) if replayable(&error) => {
                let fresh = self.connections.open()?;
                let mut client = LevelOneChannel(fresh);
                let auth = self.auth.clone().ok_or_else(|| {
                    ChannelError::Local(
                        "the workspace client has no credential to present after negotiation"
                            .to_owned(),
                    )
                })?;
                client.authenticate(auth)?;
                let outcome = client.send(intent)?;
                self.client = client;
                // The replacement presented the credential itself. Without this
                // the next request would present it again on an already
                // authenticated connection, and the host would refuse it.
                self.presented = true;
                Ok(outcome)
            }
            Err(error) => Err(error),
        }
    }
}

/// Whether a failure may be replayed under the same request id: the delivery is
/// indeterminate, so the host answers from its own record rather than admitting
/// again.
fn replayable(error: &ChannelError) -> bool {
    matches!(
        error,
        ChannelError::Unknown {
            replayable: true,
            ..
        }
    )
}

/// Forward one non-interactive command and drive it to its terminal outcome.
///
/// The returned string is the command's own summary for the CLI entrypoint; its
/// bytes were already written to the caller's sinks.
pub(crate) fn forward(
    published: &PublishedGuestContext,
    guest: &mut dyn GuestReader,
    opener: &mut dyn ChannelOpener,
    verified: VerifiedWorkspaceContext,
    live: LiveGuestRun,
    cwd: &str,
    selector: Selector,
    args: Vec<String>,
    wants_json: bool,
) -> Result<String, RunnerError> {
    let connections = GuestConnections {
        opener,
        verified,
        live,
    };
    // Order is the contract's: the pinned peer check and negotiation happen in
    // `connect`, and the credential is read from guest storage only after that,
    // so a mispinned peer causes no credential to be read, let alone sent.
    let mut session = Session::connect(Box::new(connections)).map_err(ChannelError::into_runner)?;
    let auth = super::load_auth(guest, &published.document, &super::client_id()?)?;
    session.authenticate_with(auth);
    let body = channel::InvocationBody {
        selector,
        cwd: cwd.to_owned(),
        target: channel::Target::Host,
        args,
        output: if wants_json {
            channel::OutputMode::Json
        } else {
            channel::OutputMode::Text
        },
    };
    let intent = Intent::new(channel::RequestBody::Invoke(body));
    let mut sinks = ProcessSinks::new(!wants_json);
    drive(&mut session, &intent, &mut sinks, wants_json)
}

/// The protocol loop over a forwarder's connection.
fn drive(
    channel: &mut dyn ForwardChannel,
    intent: &Intent,
    sinks: &mut dyn CommandSinks,
    wants_json: bool,
) -> Result<String, RunnerError> {
    let mut delivered = Delivered::default();

    // Admission is not success: only the terminal outcome decides the result.
    let accepted = channel.replay(intent).map_err(ChannelError::into_runner)?;
    let run =
        match accepted {
            ChannelOutcome::Accepted {
                run,
                placement: RunPlacement::Host,
            } => run,
            ChannelOutcome::Accepted { placement, .. } => {
                return Err(RunnerError::task_invocation(format!(
                    "the workspace host admitted this command at a {} placement; a \
                 forwarded command runs on the host and this client will not adopt another \
                 placement",
                    placement_label(placement)
                )))
            }
            // The intent's own run had already closed and its outcome is the answer,
            // but the response names no run, so this connection cannot read that
            // run's retained output. Level 1 has no reconnect, so a replayed
            // admission is unknown rather than a success with missing output.
            ChannelOutcome::Completed { .. } => return Err(RunnerError::task_invocation(
                "the workspace host answered this command from an earlier recorded run, and the \
                 replay names no run this connection could read the output of; the command's \
                 output and result are unknown",
            )),
            other => {
                return Err(RunnerError::task_invocation(format!(
                    "the workspace host answered an admission with {}; whether it admitted is \
                 unknown",
                    outcome_label(&other)
                )))
            }
        };

    // Take the run's owning attachment while it is live, so this connection's
    // end is the kernel's owner-loss stop for exactly this run. It is the only
    // attachment this client opens: Level 1 has no second terminal.
    let attach = Intent::attach(
        run,
        vec![StreamKind::Stdout, StreamKind::Stderr],
        BTreeMap::new(),
    );
    match channel.client().send(&attach) {
        Ok(ChannelOutcome::Attached(_)) => {}
        Ok(other) => {
            return Err(RunnerError::task_invocation(format!(
                "the workspace host answered the owning attachment with {}; the forwarded \
                 command's custody is unknown",
                outcome_label(&other)
            )))
        }
        // A refused owning attachment is only harmless when the run is already
        // over: then there is nothing left to cancel and the recorded outcome is
        // the answer. A live run with no custody would outlive this connection,
        // so it is never adopted silently.
        Err(refused) => {
            return match channel.replay(intent) {
                Ok(ChannelOutcome::Completed { outcome, exit }) => {
                    let (outcome, status) = run_from_outcome(&outcome, exit)?;
                    drain(channel.client(), run, sinks, &mut delivered)?;
                    report(outcome, status, &delivered, wants_json)
                }
                Ok(_) => Err(RunnerError::task_invocation(format!(
                    "the workspace host refused this connection's owning attachment to run {run} \
                     while the run is still live, so its end could not cancel the command: {}",
                    refused.into_runner()
                ))),
                Err(error) => Err(error.into_runner()),
            };
        }
    }

    // Deliver the run's captured output while it runs, and keep asking for its
    // terminal outcome on the same connection. Output comes from the session's
    // retained log; the replay is only the outcome check.
    let mut completed: Option<(serde_json::Value, ExitStatus)> = None;
    loop {
        let drained = poll_output(channel.client(), run, sinks, &mut delivered)?;
        if let Some((outcome, exit)) = completed.take() {
            if drained {
                let (outcome, status) = run_from_outcome(&outcome, exit)?;
                return report(outcome, status, &delivered, wants_json);
            }
            // The run has closed, so its retained log is final: keep reading
            // until a turn delivers nothing, so the tail is not lost.
            completed = Some((outcome, exit));
            continue;
        }
        if delivered.turns % OUTCOME_CHECK_EVERY == 0 {
            match channel.replay(intent).map_err(ChannelError::into_runner)? {
                ChannelOutcome::Accepted { .. } => {}
                ChannelOutcome::Completed { outcome, exit } => {
                    completed = Some((outcome, exit));
                    continue;
                }
                other => {
                    return Err(RunnerError::task_invocation(format!(
                        "the workspace host answered an outcome check with {}; the \
                         forwarded command's result is unknown",
                        outcome_label(&other)
                    )))
                }
            }
        }
        std::thread::sleep(OUTPUT_POLL);
    }
}

/// Read the run's final retained output after its outcome is known.
fn drain(
    client: &mut dyn ChannelRequest,
    run: RunId,
    sinks: &mut dyn CommandSinks,
    delivered: &mut Delivered,
) -> Result<(), RunnerError> {
    while !poll_output(client, run, sinks, delivered)? {}
    Ok(())
}

/// One bounded output read, delivered to the command's sinks.
///
/// `true` means both streams were empty. Data is only advanced past a stream's
/// position after its sink accepted it, so a failed write consumes nothing.
fn poll_output(
    client: &mut dyn ChannelRequest,
    run: RunId,
    sinks: &mut dyn CommandSinks,
    delivered: &mut Delivered,
) -> Result<bool, RunnerError> {
    let op = output_operation(run, delivered.cursors);
    let intent = Intent::new(channel::RequestBody::Observe(channel::ObserveBody {
        op,
        target: Some(channel::Target::Host),
    }));
    delivered.turns = delivered.turns.wrapping_add(1);
    let value = match client.send(&intent).map_err(ChannelError::into_runner)? {
        ChannelOutcome::Observed(value) => value,
        other => {
            return Err(RunnerError::task_invocation(format!(
                "the workspace host answered a captured-output read with {}; the forwarded \
                 command's output is unknown",
                outcome_label(&other)
            )))
        }
    };
    let read: OutputRead = serde_json::from_value(value).map_err(|_| {
        RunnerError::task_invocation(
            "the workspace host's captured-output read is not an output document; the forwarded \
             command's output is unknown",
        )
    })?;
    if read.run != run {
        return Err(RunnerError::task_invocation(format!(
            "the workspace host answered a captured-output read for run {} when this command \
             admitted run {run}",
            read.run
        )));
    }
    deliver(&read.stdout, 0, sinks, delivered, |sinks, bytes| {
        sinks.stdout(bytes)
    })?;
    deliver(&read.stderr, 1, sinks, delivered, |sinks, bytes| {
        sinks.stderr(bytes)
    })?;
    Ok(read.stdout.is_empty() && read.stderr.is_empty())
}

/// Deliver one stream's bounded read to its sink and advance its position.
fn deliver(
    stream: &StreamRead,
    index: usize,
    sinks: &mut dyn CommandSinks,
    delivered: &mut Delivered,
    sink: impl FnOnce(&mut dyn CommandSinks, &[u8]) -> std::io::Result<()>,
) -> Result<(), RunnerError> {
    if stream.gap.is_some() || stream.truncated {
        delivered.incomplete = true;
    }
    if stream.from != delivered.cursors[index] {
        return Err(RunnerError::task_invocation(format!(
            "the workspace host continued a captured stream at {} but this command had consumed \
             {}; the forwarded command's output is incomplete",
            stream.from, delivered.cursors[index]
        )));
    }
    let bytes = stream
        .decode()
        .map_err(ChannelError::from_refusal)
        .map_err(ChannelError::into_runner)?;
    if bytes.is_empty() {
        return Ok(());
    }
    // Acknowledge only after the sink accepted every byte.
    sink(sinks, &bytes).map_err(|error| {
        RunnerError::task_invocation(format!(
            "a forwarded command's output could not be written ({}); its output and result are \
             unknown",
            error.kind()
        ))
    })?;
    delivered.cursors[index] = stream.next;
    Ok(())
}

/// Map a completed admission to the outcome the caller reports.
fn run_from_outcome(
    outcome: &serde_json::Value,
    exit: ExitStatus,
) -> Result<(serde_json::Value, ExitStatus), RunnerError> {
    let status = recorded_status(exit, outcome).ok_or_else(|| {
        RunnerError::task_invocation(
            "the workspace host recorded no terminal status for this run, so whether the command \
             succeeded is unknown; inspect it from the host CLI",
        )
    })?;
    Ok((outcome.clone(), status))
}

/// Report a command whose output and outcome have both been established.
fn report(
    outcome: serde_json::Value,
    status: ExitStatus,
    delivered: &Delivered,
    wants_json: bool,
) -> Result<String, RunnerError> {
    if delivered.incomplete {
        return Err(RunnerError::task_invocation(
            "the workspace host retained only part of this command's output (a rotated or \
             truncated capture), so the command's output is incomplete and its result is not \
             reported as success",
        ));
    }
    if wants_json {
        return Ok(serde_json::json!({
            "schema": "effigy.workspace-client.v1",
            "outcome": outcome,
            "status": {
                "code": status.code,
                "signal": status.signal,
            },
        })
        .to_string());
    }
    match (status.code, status.signal) {
        // The command already wrote its output. Exit with its own status and
        // add nothing, exactly as an attached host run does.
        (Some(0), _) => Ok(String::new()),
        (Some(code), _) => Err(RunnerError::RunSettled { code }),
        // A signalled command has no exit code. The shell convention preserves
        // a real status for the caller and adds no invented output.
        (None, Some(signal)) => Err(RunnerError::RunSettled { code: 128 + signal }),
        (None, None) => Err(RunnerError::task_invocation(
            "the workspace host recorded no terminal status for this run",
        )),
    }
}

/// The session's Level 1 stream endpoint.
///
/// Level 1 publishes output through the captured-output observe, so this
/// endpoint publishes no stream frames for a forwarded run. It answers `resume`
/// for such a run so the connection's *owning* attachment can open (that
/// attachment is the run's custody, not a read), and it refuses terminal input
/// and resize by name. A run this session holds a #485 terminal for goes to
/// [`TerminalCustody`] unchanged, so the interactive terminal path is not
/// redefined here.
pub(crate) struct LevelOneStreams {
    custody: TerminalCustody,
}

impl LevelOneStreams {
    pub(crate) fn new(custody: TerminalCustody) -> Self {
        Self { custody }
    }

    /// The terminal custody this endpoint still serves unchanged.
    pub(crate) fn custody(&self) -> &TerminalCustody {
        &self.custody
    }
}

impl Streams for LevelOneStreams {
    fn next_chunk(&mut self, attachment: Option<&Attachment>) -> Option<StreamChunk> {
        self.custody.next_chunk(attachment)
    }

    fn resume(&mut self, run: RunId, cursors: &Cursors) -> Option<StreamStart> {
        if self.custody.owns(run) {
            return self.custody.resume(run, cursors);
        }
        // No terminal for this run: the attach opens only so this connection
        // owns the run's lifetime. It publishes no stream, and a cursor is not
        // meaningful because no retained stream is carried here.
        cursors.is_fresh().then(StreamStart::new)
    }

    fn start(&mut self, attachment: &Attachment, start: &StreamStart) {
        if self.custody.owns(attachment.run()) {
            self.custody.start(attachment, start);
        }
    }

    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        if self.custody.owns(attachment.run()) {
            return self.custody.input(attachment, bytes);
        }
        let _ = bytes;
        Err(Refusal::forbidden(InteractiveRequest::Input.refusal()))
    }

    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        if self.custody.owns(attachment.run()) {
            return self.custody.resize(attachment, size);
        }
        let _ = size;
        Err(Refusal::forbidden(InteractiveRequest::Resize.refusal()))
    }
}

#[cfg(test)]
#[path = "noninteractive_stream/tests.rs"]
mod tests;
