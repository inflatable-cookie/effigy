//! Journal-first local process custody (§5.7). No signal handlers live here.
mod anchor;
mod canonical_output;
pub(crate) mod capture;
pub(crate) mod merged;
mod styled_tap;
pub use canonical_output::CanonicalOutput;
pub use merged::{FileIdentity, MergedLog, PlainLog, PlainRead};
pub(crate) mod platform;
mod terminal;
/// Signal one external process only after the caller's fresh authority check.
pub fn signal_verified_process(
    identity: &ProcessIdentity,
    signal: i32,
    authority: impl FnOnce() -> bool,
) -> Observed<SignalDelivered> {
    platform::signal_checked(identity, signal, authority)
}
pub use capture::{read_styles, Stream};
pub(crate) mod spawn;
use crate::ctx::{CleanupContext, EffectContext, OperationContext};
use crate::ident::ProcessIdentity;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::run::{Effect, EffectSpec, EffectState, EffectTicket, Run, RunError, Settlement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const KIND: &str = "kernel:process";
/// Beyond its declared grace, a stop still drains bounded capture, reaps the
/// leader and stops the anchor. The margin bounds those; it is never a second
/// grace and it never replaces observing the group (effigy#357).
pub const STOP_MARGIN: Duration = Duration::from_secs(30);
/// The descriptor slot a spawn hands its optional one-shot reader to (K12).
/// It sits above the capability reader (9) and cleanup pin (10) slots, so a
/// caller can hand one short-lived payload to its own child without either
/// showing in argv or outliving the read. The child owns the copy; the parent
/// keeps its own descriptor and closes it when the spawn returns.
pub const HANDOFF_FD: i32 = 11;

/// Reported when a group's closure cannot be positively established: an
/// inventory row vanished or moved inside the round, so a member forked and
/// orphaned in that window cannot be excluded. The anchor stays alive as the
/// group's evidence and the group is never signalled on a guess. This is a best
/// effort observation, not a proof (effigy#520; contract 052).
pub const UNPROVEN_GROUP_CLOSURE: &str = "unproven group closure";
#[derive(Clone)]
pub struct SecretValue(Vec<u8>);
impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
impl SecretValue {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0
    }
    /// Whether the value is one line: line-framed transports (an env file)
    /// cannot carry a newline. Reveals nothing else about the value.
    pub fn is_single_line(&self) -> bool {
        !self.0.contains(&b'\n')
    }
    pub fn new(bytes: Vec<u8>) -> io::Result<Self> {
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid secret",
            ));
        }
        Ok(Self(bytes))
    }
}
#[derive(Debug, Clone)]
pub enum EnvValue {
    Plain(OsString),
    Secret(SecretValue),
}
#[derive(Debug, Clone)]
pub enum Lifetime {
    Finite(Duration),
    Open(crate::ctx::OpenReason),
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Terminal {
    None,
    /// Close stdin and discard both output streams before the child executes.
    Discard,
    /// The child's stdin is a pipe whose write end the owner keeps, so an
    /// engine can send it framed messages. The owner can write bounded bytes
    /// and the child sees EOF when the owner drops it. Stdout and stderr are
    /// captured as for `None`.
    Pipe,
    /// Managed pane capture: persist merged plain text and expose redacted style data.
    Styled {
        pty: bool,
    },
    Pty,
    /// The owner's terminal as the child's stdin, with the child's own process
    /// group as the terminal's foreground group while it runs (a passphrase or
    /// a `sudo` prompt). The lease ends when that child's exit is observed, so
    /// one owner can run foreground children one after another.
    Foreground,
    /// The owner's own stdin, and its stdout and stderr unless `capture`:
    /// the operator's terminal or pipe, never an Effigy sink. The child's
    /// group takes the terminal only when the owner is that terminal's
    /// foreground group. Captured streams are bounded and redacted as usual.
    Inherit {
        capture: bool,
    },
}
#[derive(Clone)]
pub struct ProcessSpec {
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    /// Complete spawn environment; the parent environment is never modified.
    pub env: BTreeMap<OsString, EnvValue>,
    pub grace: Duration,
    pub lifetime: Lifetime,
    pub terminal: Terminal,
    pub label: String,
}
impl ProcessSpec {
    /// Shared spawn preflight: secret bytes never reach ps-visible argv.
    pub(crate) fn validated_secrets(&self) -> io::Result<Vec<Vec<u8>>> {
        let secrets: Vec<_> = self
            .env
            .values()
            .filter_map(|v| match v {
                EnvValue::Secret(v) => Some(v.0.clone()),
                _ => None,
            })
            .collect();
        if self.argv.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty argv"));
        }
        if let Some((name, _)) = self.env.iter().find(|(_, value)| match value {
            EnvValue::Secret(value) => self.argv.iter().any(|arg| {
                arg.as_bytes()
                    .windows(value.bytes().len())
                    .any(|part| part == value.bytes())
            }),
            EnvValue::Plain(_) => false,
        }) {
            let name: String = name
                .to_string_lossy()
                .chars()
                .take(128)
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        ch
                    } else {
                        '?'
                    }
                })
                .collect();
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("secret in argv from classified environment key `{name}`; pass the value through environment access")));
        }
        Ok(secrets)
    }
}
impl std::fmt::Debug for ProcessSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcessSpec { argv/env/label: [REDACTED] }")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessReceipt {
    pub identity: ProcessIdentity,
    pub pgid: u32,
    pub nonce: String,
    pub grace_millis: u64,
}
// Persisted extension of the public process identity receipt. Flattening keeps
// old receipts readable and avoids changing fixture/probe custody's format.
#[derive(Debug, Serialize, Deserialize)]
struct ProcessRecord {
    #[serde(flatten)]
    process: ProcessReceipt,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    anchor: Option<ProcessIdentity>,
}
// A distinct outer envelope makes a discard receipt unreadable to an older
// process-record reader. In particular, an old sentinel must not acknowledge
// a new output policy it cannot interpret.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum StoredProcessRecord {
    Discard { discard_v1: ProcessRecord },
    Legacy(ProcessRecord),
}
impl StoredProcessRecord {
    fn into_record(self) -> ProcessRecord {
        match self {
            Self::Discard { discard_v1 } => discard_v1,
            Self::Legacy(record) => record,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub code: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closure;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    Complete,
    Truncated(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDelivered {
    pub pid: u32,
    pub group: bool,
    pub signal: i32,
}
#[derive(Debug)]
pub struct StopReport {
    pub leader: Observed<Exit>,
    pub group: Observed<Closure>,
    pub capture: CaptureState,
    pub signals: Vec<SignalDelivered>,
    pub residue: Vec<UnknownReason>,
}
#[derive(Debug)]
pub enum SpawnError {
    Refused(String),
    Run(RunError),
    Io(io::Error),
    Unrecorded(RunError),
}
impl From<io::Error> for SpawnError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<RunError> for SpawnError {
    fn from(e: RunError) -> Self {
        Self::Run(e)
    }
}
impl std::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(reason) => write!(f, "the process was not started ({reason})"),
            Self::Run(error) => error.fmt(f),
            Self::Io(error) => error.fmt(f),
            Self::Unrecorded(error) => write!(
                f,
                "the process started but its record could not be kept ({error}); it is residue, and recovery reconciles it before the run settles"
            ),
        }
    }
}
impl std::error::Error for SpawnError {}
#[derive(Debug)]
pub struct PartialSpawn {
    pub primary: SpawnError,
    pub cleanup: Vec<StopReport>,
}
/// A caller's last step between a spawned child's recorded receipt and its
/// `go`. An engine uses it to deliver evidence about the exact group that is
/// about to exec. A refusal stops the child before it can run: it is still
/// behind the barrier, so it exits without exec and its group is reaped.
pub trait SpawnGate {
    fn before_go(&mut self, receipt: &ProcessReceipt) -> Result<(), SpawnError>;
}

#[derive(Debug)]
pub struct Custody;
#[derive(Debug)]
pub struct OwnedProcess {
    pub(crate) core: Arc<Mutex<ProcessCore>>,
    ticket: EffectTicket,
}
#[derive(Debug)]
pub(crate) struct ProcessCore {
    receipt: ProcessReceipt,
    anchor: Option<ProcessIdentity>,
    /// stdout and stderr logs, fed from the one merged, sanitized redaction.
    logs: [Option<capture::Log>; 2],
    canonical_output: Option<CanonicalOutput>,
    output: VecDeque<u8>,
    /// stdout and stderr kept apart for engines that report them separately.
    /// Each is the redacted projection of its own stream over the merged,
    /// sanitized result: a projection can join halves that arrival order kept
    /// apart, so it is redacted again. The merged output and tail are the
    /// canonical capture; two projections are never concatenated into one.
    streams: [VecDeque<u8>; 2],
    queue_max: usize,
    stdout: Option<OwnedFd>,
    stderr: Option<OwnedFd>,
    /// The owner's side of a `Terminal::Pipe` child's stdin.
    stdin: Option<OwnedFd>,
    /// Sanitize then redact, once, over merged arrival order: every sink below
    /// (per-stream logs, queues, merged output, tail) is a projection of it.
    pipeline: OutputPipeline,
    /// The canonical captured tail: the last bytes of that same merged,
    /// sanitized and redacted arrival order. A reader never re-groups streams
    /// to rebuild it.
    tail: VecDeque<u8>,
    tail_max: usize,
    capture: CaptureState,
    exit: Option<Exit>,
    reaped: bool,
    deadline: Option<Instant>,
    terminal: Option<spawn::TerminalLease>,
    pty: bool,
    registration: Option<Arc<Mutex<crate::remote::stream::RegistrationStream>>>,
    capture_eof_sent: bool,
}
/// A bounded live projection of canonical output, with explicit queue loss.
#[derive(Debug, Clone, Default)]
pub struct StyledOutput {
    pub chunks: Vec<effigy_core::terminal_text::StyledChunk>,
    pub truncated: bool,
}
/// The one path from a secret-bearing process's merged raw output to every
/// sink (K12): terminal controls are removed first, so the redactor sees
/// exactly the visible bytes a front end would show, then redaction runs.
/// Splitting that order, or normalizing after redaction, reopens the split
/// secret: `super` CSI `secret` is one value to a terminal and two
/// non-matching runs to a redactor over the raw bytes.
#[derive(Debug)]
struct OutputPipeline {
    /// `Some` only for a secret-bearing process. An ordinary process keeps its
    /// terminal colour, exactly as before this seam existed.
    sanitizer: Option<terminal::TerminalSanitizer>,
    redactor: Redactor<OutputTag>,
    plain: bool,
    /// One redactor per stream, present only for a secret-bearing process.
    ///
    /// Splitting the merged result by origin stream is a byte removal, and a
    /// removal can join two halves of a secret that the merged redaction
    /// correctly left alone, because the other stream's *visible* bytes sat
    /// between them (`super` on stdout, `noise` on stderr, `secret` on
    /// stdout). Each projection is therefore redacted again over its own byte
    /// stream before it reaches a stored log or a queue. Keeping both streams
    /// in arrival order is the only capture that cannot be reordered, so the
    /// merged output and the merged tail stay canonical, and a consumer that
    /// wants both streams reads them, never a concatenation of projections.
    projections: Option<[Redactor; 2]>,
}
impl OutputPipeline {
    fn new(secrets: Vec<Vec<u8>>) -> Self {
        Self::for_capture(secrets, false)
    }
    fn for_capture(mut secrets: Vec<Vec<u8>>, plain: bool) -> Self {
        if plain {
            secrets.extend(
                secrets
                    .iter()
                    .map(|secret| {
                        terminal::visible(secret)
                            .into_iter()
                            .filter(|byte| *byte != b'\r')
                            .collect::<Vec<_>>()
                    })
                    .filter(|secret| !secret.is_empty())
                    .collect::<Vec<_>>(),
            );
        }
        let secret_bearing = !secrets.is_empty();
        Self {
            sanitizer: (secret_bearing || plain).then(terminal::TerminalSanitizer::new),
            plain,
            projections: (secret_bearing && !plain).then(|| {
                [
                    Redactor::new(secrets.clone()),
                    Redactor::new(secrets.clone()),
                ]
            }),
            redactor: Redactor::with_tags(secrets),
        }
    }
    /// Sanitize, then redact, one merged arrival-order slice. Both steps keep
    /// each byte's origin tag.
    fn push_tagged(&mut self, bytes: &[(u8, u8)], eof: bool) -> Vec<(OutputTag, u8)> {
        self.push_terminal(bytes, eof, eof)
    }
    fn push_terminal(
        &mut self,
        bytes: &[(u8, u8)],
        terminal_eof: bool,
        eof: bool,
    ) -> Vec<(OutputTag, u8)> {
        match self.sanitizer.as_mut() {
            Some(sanitizer) => {
                let mut visible = sanitizer.push_styled(bytes, terminal_eof);
                if self.plain {
                    visible.retain(|(_, byte)| *byte != b'\r');
                }
                // Empty is not a no-op: EOF still flushes the redactor.
                self.redactor.push_tagged(&visible, eof)
            }
            None => {
                let tagged: Vec<_> = bytes
                    .iter()
                    .map(|(stream, byte)| {
                        (
                            OutputTag {
                                stream: *stream,
                                ..OutputTag::default()
                            },
                            *byte,
                        )
                    })
                    .collect();
                self.redactor.push_tagged(&tagged, eof)
            }
        }
    }
    /// Each stream's own safe projection of one merged result. A stream sink
    /// is only ever written from this, never from a filtered merged slice.
    fn project(&mut self, merged: &[(OutputTag, u8)], eof: bool) -> [Vec<u8>; 2] {
        let mut raw: [Vec<u8>; 2] = Default::default();
        for &(tag, byte) in merged {
            if let Some(part) = raw.get_mut(usize::from(tag.stream)) {
                part.push(byte);
            }
        }
        let Some(projections) = self.projections.as_mut() else {
            return raw;
        };
        for (stream, redactor) in projections.iter_mut().enumerate() {
            // The projection is its own stream of bytes, so it is redacted as
            // one: a secret its own bytes form is caught even when arrival
            // order interleaved them with the other stream's.
            raw[stream] = redactor.push(&raw[stream], eof);
        }
        raw
    }
}
pub(crate) trait RedactionTag: Copy {
    fn masked(self) -> Self;
}
impl RedactionTag for u8 {
    fn masked(self) -> Self {
        self
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct OutputTag {
    stream: u8,
    style: effigy_core::terminal_text::Style,
}
impl RedactionTag for OutputTag {
    fn masked(self) -> Self {
        Self {
            stream: self.stream,
            ..Self::default()
        }
    }
}
pub(crate) struct Redactor<T: RedactionTag = u8> {
    /// `None` when no reader-safe byte is absent from every secret, so no
    /// replacement can be emitted that a reader cannot join across: nothing is
    /// redacted and the capture stops (`blocked`) rather than leak.
    display: Option<RedactionPass<T>>,
    final_check: Option<RedactionPass<T>>,
}
impl<T: RedactionTag> std::fmt::Debug for Redactor<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Redactor([REDACTED])")
    }
}
impl<T: RedactionTag> Redactor<T> {
    fn with_tags(mut secrets: Vec<Vec<u8>>) -> Self {
        // A PTY's output processing expands LF to CRLF. Match that display
        // representation too, before it reaches logs, tails or a front end.
        let terminal = secrets
            .iter()
            .filter(|secret| secret.contains(&b'\n'))
            .map(|secret| {
                let mut displayed = Vec::with_capacity(secret.len());
                for &byte in secret {
                    if byte == b'\n' {
                        displayed.push(b'\r');
                    }
                    displayed.push(byte);
                }
                displayed
            })
            .collect::<Vec<_>>();
        secrets.extend(terminal);
        // A secret can itself carry a terminal control. The sanitizer removes
        // it, so the bytes a reader shows are that value's visible form; match
        // it too, or stripping a control out of a secret would leave a visible
        // remainder no pattern covers (K12).
        secrets.extend(
            secrets
                .iter()
                .map(|secret| terminal::visible(secret))
                .filter(|visible| !visible.is_empty())
                .collect::<Vec<_>>(),
        );
        secrets.sort();
        secrets.dedup();
        // A secret can itself contain the display marker, or a replacement can
        // join adjacent bytes into another secret. A second streaming pass
        // replaces such an occurrence with one byte: a byte *no reader drops*
        // and no secret contains. Both halves matter. A control byte (a NUL,
        // say) is dropped by a reader, which joins the bytes either side of it
        // and can rebuild the very secret this pass removed; a byte a secret
        // contains could be part of one. With a byte in neither class, the
        // replacement can neither hold a secret nor be spanned by one.
        let replacement = final_replacement(&secrets);
        Self {
            display: replacement
                .map(|_| RedactionPass::new(secrets.clone(), b"[REDACTED]".to_vec())),
            final_check: replacement.map(|byte| RedactionPass::new(secrets, vec![byte])),
        }
    }
    fn has_pending(&self) -> bool {
        self.display
            .as_ref()
            .is_some_and(|pass| !pass.pending.is_empty())
            || self
                .final_check
                .as_ref()
                .is_some_and(|pass| !pass.pending.is_empty())
    }
    /// Whether no reader-safe replacement byte exists for this secret set, so
    /// nothing can be emitted at all.
    pub(crate) fn blocked(&self) -> bool {
        self.display.is_none()
    }
    /// Redacts `(stream, byte)` pairs in arrival order. A replacement takes
    /// the stream of the secret's first byte, so a secret split across
    /// stdout and stderr is redacted whole and attributed to one stream.
    pub(crate) fn push_tagged(&mut self, bytes: &[(T, u8)], eof: bool) -> Vec<(T, u8)> {
        let (Some(display), Some(final_check)) = (self.display.as_mut(), self.final_check.as_mut())
        else {
            return Vec::new();
        };
        let shown = display.push(bytes, eof);
        final_check.push(&shown, eof)
    }
}

impl Redactor<u8> {
    pub(crate) fn new(secrets: Vec<Vec<u8>>) -> Self {
        Self::with_tags(secrets)
    }
    pub(crate) fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        let tagged: Vec<(u8, u8)> = bytes.iter().map(|byte| (0, *byte)).collect();
        self.push_tagged(&tagged, eof)
            .into_iter()
            .map(|(_, byte)| byte)
            .collect()
    }
}

/// The byte a re-formed secret is replaced with: the first value a reader keeps
/// and no secret contains, preferred in an order that keeps output readable.
/// Every control is either dropped by a reader or moved by a renderer, so only
/// printable ASCII is a candidate. `None` when the secrets cover them all: no
/// replacement could then be both safe and readable.
fn final_replacement(secrets: &[Vec<u8>]) -> Option<u8> {
    const PREFERRED: &[u8] = b"~`^|\\{}<>$&*?!;:+=@%#/\"',()[]";
    let absent = |byte: u8| !secrets.iter().any(|secret| secret.contains(&byte));
    PREFERRED
        .iter()
        .copied()
        .chain((0x20..=0x7e).filter(|byte| !PREFERRED.contains(byte)))
        .find(|byte| absent(*byte))
}
struct RedactionPass<T: RedactionTag> {
    secrets: Vec<Vec<u8>>,
    pending: Vec<(T, u8)>,
    replacement: Vec<u8>,
}
impl<T: RedactionTag> RedactionPass<T> {
    fn new(mut secrets: Vec<Vec<u8>>, replacement: Vec<u8>) -> Self {
        secrets.sort_by_key(|v| std::cmp::Reverse(v.len()));
        Self {
            secrets,
            pending: Vec::new(),
            replacement,
        }
    }
    fn push(&mut self, bytes: &[(T, u8)], eof: bool) -> Vec<(T, u8)> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < self.pending.len() {
            let rest = &self.pending[pos..];
            // Only a suffix that could still complete a known value needs
            // lookahead. Long inherited values must not stall unrelated live
            // lines until max-secret-length bytes or process EOF arrive.
            if !eof
                && self.secrets.iter().any(|secret| {
                    rest.len() < secret.len()
                        && rest
                            .iter()
                            .zip(secret)
                            .all(|((_, byte), expected)| byte == expected)
                })
            {
                break;
            }
            if let Some(secret) = self.secrets.iter().find(|s| {
                rest.len() >= s.len() && rest.iter().zip(s.iter()).all(|((_, b), c)| b == c)
            }) {
                let stream = rest[0].0.masked();
                out.extend(self.replacement.iter().map(|byte| (stream, *byte)));
                pos += secret.len();
            } else {
                out.push(rest[0]);
                pos += 1;
            }
        }
        self.pending.drain(..pos);
        out
    }
}
fn evidence() -> Evidence {
    Evidence::new("kernel process custody".into(), None).expect("fixed source")
}
fn unknown(e: impl std::fmt::Display) -> UnknownReason {
    UnknownReason::Inconsistent(e.to_string())
}
/// A spent window is not evidence that contradicts itself: the stop says what
/// it waited for and how long (effigy#370).
fn timed_out(phase: impl std::fmt::Display) -> UnknownReason {
    UnknownReason::TimedOut {
        phase: phase.to_string(),
    }
}
fn lock(core: &Mutex<ProcessCore>) -> std::sync::MutexGuard<'_, ProcessCore> {
    core.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
/// The two intent stores share the same spawn barrier and process mechanics.
/// This private seam cannot grant an engine raw spawn or signal authority.
trait ProcessOwner {
    fn id(&self) -> crate::id::RunId;
    fn fence(&self) -> &crate::run::Fence;
    fn has_sentinel(&self) -> bool;
    fn closure_pin(&self) -> Result<Option<crate::closure::ClosurePin>, RunError> {
        Ok(None)
    }
    fn intend_process(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError>;
    fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError>;
    fn ack_process(&mut self, ticket: &EffectTicket) -> Result<(), RunError>;
    fn anchor_image(&self) -> Result<crate::supervise::anchor::Image, RunError>;
    fn capture_log(
        &self,
        sequence: i64,
        stream: Stream,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Option<capture::Log>>;
    fn retain_process(&mut self, sequence: i64, core: Arc<Mutex<ProcessCore>>);
}
impl ProcessOwner for Run<'_> {
    fn closure_pin(&self) -> Result<Option<crate::closure::ClosurePin>, RunError> {
        self.checkout_closure_pin()
    }
    fn id(&self) -> crate::id::RunId {
        self.id()
    }
    fn fence(&self) -> &crate::run::Fence {
        self.fence()
    }
    fn has_sentinel(&self) -> bool {
        self.has_sentinel()
    }
    fn intend_process(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.verify_custody()?;
        self.intend_process(spec)
    }
    fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
        self.applied(ticket, receipt)
    }
    fn ack_process(&mut self, ticket: &EffectTicket) -> Result<(), RunError> {
        self.acknowledge_checkout_process(ticket)
    }
    fn anchor_image(&self) -> Result<crate::supervise::anchor::Image, RunError> {
        self.anchor_image()
    }
    fn capture_log(
        &self,
        sequence: i64,
        stream: Stream,
        limits: &crate::ctx::Limits,
    ) -> io::Result<Option<capture::Log>> {
        self.capture_log(sequence, stream, limits)
    }
    fn retain_process(&mut self, sequence: i64, core: Arc<Mutex<ProcessCore>>) {
        self.retain_process(sequence, core);
    }
}
impl ProcessOwner for crate::run::ForegroundRun {
    fn closure_pin(&self) -> Result<Option<crate::closure::ClosurePin>, RunError> {
        self.closure_pin().map(Some)
    }
    fn id(&self) -> crate::id::RunId {
        self.record().incarnation().run
    }
    fn fence(&self) -> &crate::run::Fence {
        self.record().fence()
    }
    fn has_sentinel(&self) -> bool {
        self.has_guard()
    }
    fn intend_process(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.record_mut().intend_process(spec)
    }
    fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
        self.record_mut().applied(ticket, receipt)
    }
    fn ack_process(&mut self, ticket: &EffectTicket) -> Result<(), RunError> {
        self.ack_process(ticket)
    }
    fn anchor_image(&self) -> Result<crate::supervise::anchor::Image, RunError> {
        self.anchor_image()
    }
    fn capture_log(
        &self,
        _: i64,
        _: Stream,
        _: &crate::ctx::Limits,
    ) -> io::Result<Option<capture::Log>> {
        Ok(None)
    }
    fn retain_process(&mut self, sequence: i64, core: Arc<Mutex<ProcessCore>>) {
        self.retain_process(sequence, core);
    }
}

/// Pure, redacted intent planning. The machine gateway admits this exact
/// process and its control socket together before either external effect.
pub(crate) fn process_intent(
    spec: &ProcessSpec,
    target: crate::run::Fence,
    required: bool,
) -> Result<EffectSpec, SpawnError> {
    let secrets = spec
        .validated_secrets()
        .map_err(|e| SpawnError::Refused(e.to_string()))?;
    let mut redact = Redactor::new(secrets);
    let label = String::from_utf8_lossy(&redact.push(spec.label.as_bytes(), true)).into_owned();
    // Only a digest and a redacted label enter the journal. Never env values.
    let digest = crate::id::Digest::of(
        &serde_json::to_vec(&serde_json::json!({
            "argv":spec.argv.iter().map(|s|s.as_bytes()).collect::<Vec<_>>(),
            "cwd":spec.cwd.as_os_str().as_bytes(),
            "env":spec.env.iter().map(|(key,value)| (key.as_bytes(), match value {
                EnvValue::Plain(v)=>v.as_bytes(), EnvValue::Secret(v)=>v.bytes()
            })).collect::<Vec<_>>(),
            "grace_ns":spec.grace.as_nanos().to_string(),
            "lifetime":format!("{:?}",spec.lifetime),"terminal":spec.terminal
        }))
        .map_err(|_| SpawnError::Refused("spec encoding".into()))?,
    );
    Ok(EffectSpec {
        target,
        kind: KIND.into(),
        payload: serde_json::json!({"digest":digest,"label":label}),
        settlement: Settlement::Release(Value::Null),
        required,
    })
}

/// Only a positively proven anchor may accompany a recorded receipt or a
/// `go`: its generation, readable custody nonce, group membership and start
/// order are all required. An absent anchor needs no proof because nothing
/// claims its identity; an unproven one is retained as a pre-exec refusal.
fn qualify_anchor(
    receipt: &ProcessReceipt,
    anchor: Option<&crate::ident::ProcessIdentity>,
) -> Result<(), SpawnError> {
    let Some(anchor) = anchor else {
        return Ok(());
    };
    if !anchor::verified(receipt, anchor) {
        return Err(SpawnError::Refused(
            "anchor identity, nonce or group unproven".into(),
        ));
    }
    Ok(())
}

/// The leader must still be the exact unreaped direct child the receipt
/// recorded, and it must not have become a zombie, before the barrier opens.
fn retained_leader(receipt: &ProcessReceipt) -> Result<(), SpawnError> {
    use crate::ident::ProcessState;
    let identity = crate::ident::process_identity(receipt.identity.pid);
    let state = crate::ident::process_state(receipt.identity.pid);
    if identity.verified() != Some(&receipt.identity)
        || !matches!(
            state,
            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _)
        )
    {
        return Err(SpawnError::Refused(
            "leader identity or state unproven before go".into(),
        ));
    }
    Ok(())
}

impl Custody {
    pub fn spawn_foreground(
        run: &mut crate::run::ForegroundRun,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            true,
            None,
            None,
            None,
        )
    }

    /// An in-memory admission holder. Its exit is not a workload obligation;
    /// the same receipt/ack barrier and guard cleanup still apply.
    pub fn spawn_auxiliary_foreground(
        run: &mut crate::run::ForegroundRun,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            false,
            None,
            None,
            None,
        )
    }

    pub fn spawn(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            true,
            None,
            None,
            None,
        )
    }
    /// A process the run keeps in custody without making its exit a workload
    /// obligation: an admission holder that runs until settle stops it. It
    /// is journalled, sentinelled and settled exactly like `spawn`.
    pub fn spawn_auxiliary(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            false,
            None,
            None,
            None,
        )
    }
    /// Acquire executable source under a Prepare run. This uses the same
    /// journal, sentinel, deadlines and settlement as ordinary process custody;
    /// it does not grant Prepare permission to perform adapter mutations.
    pub fn spawn_source(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AcquireSource,
            true,
            None,
            None,
            None,
        )
    }
    pub(crate) fn spawn_remote(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
        registration: Arc<Mutex<crate::remote::stream::RegistrationStream>>,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            Some(registration),
            crate::ctx::Capability::AdapterEffect,
            true,
            None,
            None,
            None,
        )
    }
    /// A spawn that hands its child one short-lived reader at [`HANDOFF_FD`];
    /// the caller keeps no copy after this returns. `required` distinguishes
    /// workload and auxiliary custody.
    pub fn spawn_with_handoff(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
        required: bool,
        handoff: Option<OwnedFd>,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            required,
            None,
            handoff,
            None,
        )
    }
    pub fn spawn_foreground_with_handoff(
        run: &mut crate::run::ForegroundRun,
        spec: ProcessSpec,
        cx: &EffectContext,
        required: bool,
        handoff: Option<OwnedFd>,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            required,
            None,
            handoff,
            None,
        )
    }
    /// `spawn_foreground_with_handoff` with a gate between the recorded receipt
    /// and the child's `go`. A gate refusal leaves the child unexecuted.
    pub fn spawn_foreground_gated(
        run: &mut crate::run::ForegroundRun,
        spec: ProcessSpec,
        cx: &EffectContext,
        gate: &mut dyn SpawnGate,
        handoff: Option<OwnedFd>,
    ) -> Result<OwnedProcess, SpawnError> {
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            true,
            Some(gate),
            handoff,
            None,
        )
    }
    /// Capture both child pipes into one append-only plain log. The destination
    /// is installed before the child passes its execution barrier; inherited
    /// output descriptors are refused rather than bypassing custody.
    pub fn spawn_logged_auxiliary(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
        output: CanonicalOutput,
    ) -> Result<OwnedProcess, SpawnError> {
        if !matches!(spec.terminal, Terminal::None | Terminal::Pipe) {
            return Err(SpawnError::Refused(
                "merged logs require captured pipes".into(),
            ));
        }
        if spec
            .validated_secrets()?
            .iter()
            .any(|secret| std::str::from_utf8(secret).is_err())
        {
            return Err(SpawnError::Refused(
                "plain styled capture requires UTF-8 secrets".into(),
            ));
        }
        output.validate(&spec)?;
        Self::spawn_inner(
            run,
            spec,
            cx,
            None,
            crate::ctx::Capability::AdapterEffect,
            false,
            None,
            None,
            Some(output),
        )
    }
    // Every parameter is a distinct spawn knob the public entry points set;
    // the gate is the one an engine adds between receipt and `go`.
    #[allow(clippy::too_many_arguments)]
    fn spawn_inner(
        run: &mut impl ProcessOwner,
        spec: ProcessSpec,
        cx: &EffectContext,
        registration: Option<Arc<Mutex<crate::remote::stream::RegistrationStream>>>,
        capability: crate::ctx::Capability,
        required: bool,
        gate: Option<&mut dyn SpawnGate>,
        handoff: Option<OwnedFd>,
        canonical_output: Option<CanonicalOutput>,
    ) -> Result<OwnedProcess, SpawnError> {
        // A discard receipt needs every recovery helper to understand the
        // disposition before this owner records an intent or forks. Existing
        // helper identities do not yet carry a verified image capability.
        if matches!(spec.terminal, Terminal::Discard) {
            return Err(SpawnError::Refused(
                "workspace discard helper capability unavailable".into(),
            ));
        }
        cx.require(capability)
            .map_err(|_| SpawnError::Refused(format!("spawn needs {capability:?}")))?;
        if crate::store::in_transaction() {
            return Err(SpawnError::Refused("spawn inside transaction".into()));
        }
        if !run.has_sentinel() {
            return Err(SpawnError::Refused(
                "custody requires a registered sentinel".into(),
            ));
        }
        cx.operation().check("spawn").map_err(RunError::from)?;
        let secrets = spec
            .validated_secrets()
            .map_err(|e| SpawnError::Refused(e.to_string()))?;
        if matches!(spec.terminal, Terminal::Styled { .. })
            && secrets
                .iter()
                .any(|secret| std::str::from_utf8(secret).is_err())
        {
            return Err(SpawnError::Refused(
                "managed plain capture requires UTF-8 secret provenance".into(),
            ));
        }
        let styled = matches!(spec.terminal, Terminal::Styled { .. });
        let pipeline = OutputPipeline::new(if canonical_output.is_some() || styled {
            Vec::new()
        } else {
            secrets.clone()
        });
        let ticket = run.intend_process(process_intent(&spec, run.fence().clone(), required)?)?;
        let nonce = format!("{}:{}", run.id(), ticket.sequence());
        let image = run.anchor_image()?;
        let closure = run.closure_pin()?;
        let mut pending = spawn::Blocked::fork(
            &spec,
            &nonce,
            cx.operation(),
            &image,
            closure.as_ref(),
            handoff.as_ref(),
        )?;
        // The child holds its own copy of the handoff reader: this process's
        // copy closes now, so the payload never outlives the spawn.
        drop(handoff);
        drop(closure);
        drop(image); // helper verified the image before acknowledging the barrier
        let result = (|| {
            crate::fault_point!(SpawnFork)?;
            crate::fault_point!(SpawnBeforeRecord)?;
            let identity = crate::ident::process_identity(pending.pid)
                .verified()
                .cloned()
                .ok_or_else(|| SpawnError::Refused("child identity unavailable".into()))?;
            let receipt = ProcessReceipt {
                identity,
                pgid: pending.pid,
                nonce,
                grace_millis: spec.grace.as_millis().min(u128::from(u64::MAX)) as u64,
            };
            let anchor = pending
                .anchor
                .map(|pid| {
                    crate::ident::process_identity(pid)
                        .verified()
                        .cloned()
                        .ok_or_else(|| SpawnError::Refused("anchor identity unavailable".into()))
                })
                .transpose()?;
            crate::fault_point!(AnchorSpawn)?;
            // No receipt, backup registration or go for an anchor this process
            // cannot positively prove. The child is still behind its barrier,
            // so a refusal is a pre-exec reaping, not an unknown workload.
            qualify_anchor(&receipt, anchor.as_ref())?;
            let record = ProcessRecord {
                process: receipt.clone(),
                anchor: anchor.clone(),
            };
            let record = if matches!(spec.terminal, Terminal::Discard) {
                StoredProcessRecord::Discard { discard_v1: record }
            } else {
                StoredProcessRecord::Legacy(record)
            };
            run.applied(
                &ticket,
                serde_json::to_value(record)
                    .map_err(|_| SpawnError::Refused("receipt encoding".into()))?,
            )
            .map_err(SpawnError::Unrecorded)?;
            crate::fault_point!(AnchorRecord)?;
            // The memory backup retains the receipt before go.
            run.ack_process(&ticket)?;
            // The caller's gate runs while the child is still behind the
            // barrier and before this process retains its core, so a refusal is
            // settled from the recorded receipt: the child never execed and
            // `Blocked::drop` leaves no live member of its group.
            if let Some(gate) = gate {
                gate.before_go(&receipt)?;
            }
            // The barrier has not opened yet: re-prove the exact leader and
            // anchor under the same ownership before `go`, so a death or a
            // replacement in the register/gate window cannot reach exec.
            retained_leader(&receipt)?;
            qualify_anchor(&receipt, anchor.as_ref())?;
            let (stdout, stderr, terminal) = pending.take_streams();
            let stdin = pending.take_input();
            let logs = if matches!(spec.terminal, Terminal::Discard) {
                [Ok(None), Ok(None)]
            } else {
                [Stream::Stdout, Stream::Stderr].map(|stream| {
                    run.capture_log(ticket.sequence(), stream, cx.operation().limits())
                })
            };
            let capture = if logs.iter().all(Result::is_ok) {
                CaptureState::Complete
            } else {
                CaptureState::Truncated("log open failed".into())
            };
            let [stdout_log, stderr_log] = logs;
            let (logs, canonical_output) = if styled {
                let log = stdout_log
                    .map_err(|_| SpawnError::Refused("managed plain log unavailable".into()))?
                    .ok_or_else(|| {
                        SpawnError::Refused("managed plain capture needs a durable log".into())
                    })?;
                (
                    [None, stderr_log.ok().flatten()],
                    Some(CanonicalOutput::for_run(
                        log,
                        secrets,
                        cx.operation().limits(),
                    )?),
                )
            } else {
                (
                    [stdout_log.ok().flatten(), stderr_log.ok().flatten()],
                    canonical_output,
                )
            };
            let core = Arc::new(Mutex::new(ProcessCore {
                receipt: receipt.clone(),
                anchor,
                logs,
                canonical_output,
                output: VecDeque::new(),
                streams: [VecDeque::new(), VecDeque::new()],
                queue_max: cx.operation().limits().queue_bytes,
                stdout: Some(stdout),
                stderr: Some(stderr),
                stdin,
                pipeline,
                tail: VecDeque::new(),
                tail_max: cx.operation().limits().capture_tail,
                capture,
                exit: None,
                reaped: false,
                deadline: match spec.lifetime {
                    Lifetime::Finite(budget) => Instant::now().checked_add(budget),
                    Lifetime::Open(_) => None,
                },
                terminal,
                pty: matches!(
                    spec.terminal,
                    Terminal::Pty | Terminal::Styled { pty: true }
                ),
                registration,
                capture_eof_sent: false,
            }));
            run.retain_process(ticket.sequence(), core.clone());
            pending.transfer();
            crate::fault_point!(SpawnBeforeGo)?;
            pending.go()?;
            crate::fault_point!(SpawnAfterGo)?;
            pending.executed(cx.operation())?;
            Ok(OwnedProcess { core, ticket })
        })();
        // Blocked owns the pre-exec child until go. Its drop closes the barrier
        // and reaps it on every refusal. After go the Run owns the live core.
        result
    }
    pub fn spawn_all(
        run: &mut Run<'_>,
        specs: Vec<ProcessSpec>,
        cx: &EffectContext,
    ) -> Result<Vec<OwnedProcess>, PartialSpawn> {
        let mut children: Vec<OwnedProcess> = Vec::new();
        for spec in specs {
            match Self::spawn(run, spec, cx) {
                Ok(child) => children.push(child),
                Err(primary) => {
                    let cleanup = match cx.operation().cleanup(Duration::from_secs(30)) {
                        Ok(cx) => run.rollback_processes(&cx),
                        Err(_) => Vec::new(),
                    };
                    return Err(PartialSpawn { primary, cleanup });
                }
            }
        }
        Ok(children)
    }
}
impl OwnedProcess {
    pub fn identity(&self) -> ProcessReceipt {
        lock(&self.core).receipt.clone()
    }
    /// The group anchor this launch recorded. Contract 052 settlement keeps it
    /// as positive evidence of the group identity until workload closure, so a
    /// proof must be able to observe it.
    pub fn anchor(&self) -> Option<ProcessIdentity> {
        lock(&self.core).anchor.clone()
    }
    pub fn expired(&self) -> bool {
        lock(&self.core)
            .deadline
            .is_some_and(|at| Instant::now() >= at)
    }
    pub fn take_output(&self) -> Vec<u8> {
        let mut core = lock(&self.core);
        if let Some(output) = &core.canonical_output {
            return output.take_output();
        }
        core.output.drain(..).collect()
    }
    /// Drains the separately redacted stdout and stderr queues.
    /// Drain bounded, already-redacted style data. A gap is explicit; a front
    /// may use the canonical plain file to recover it without interpreting ANSI.
    pub fn take_styled_output(&self) -> StyledOutput {
        let core = lock(&self.core);
        core.canonical_output
            .as_ref()
            .map_or_else(StyledOutput::default, CanonicalOutput::take_styled_output)
    }
    /// Managed output is one canonical stream; no per-stream regrouping.
    pub fn take_stream_output(&self) -> (Vec<u8>, Vec<u8>) {
        let mut core = lock(&self.core);
        if let Some(output) = &core.canonical_output {
            return (output.take_output(), Vec::new());
        }
        let [stdout, stderr] = &mut core.streams;
        (stdout.drain(..).collect(), stderr.drain(..).collect())
    }
    pub fn tail(&self) -> Vec<u8> {
        let core = lock(&self.core);
        if let Some(output) = &core.canonical_output {
            return output.tail();
        }
        core.tail.iter().copied().collect()
    }
    pub fn poll(&mut self) -> Observed<Exit> {
        lock(&self.core).poll()
    }
    pub fn wait(&mut self, cx: &OperationContext) -> Observed<Exit> {
        loop {
            if let Err(e) = cx.check("process wait") {
                return Observed::Unknown(unknown(format!("{e}")));
            }
            let mut core = lock(&self.core);
            if core.deadline.is_some_and(|at| Instant::now() >= at) {
                return Observed::Unknown(UnknownReason::TimedOut {
                    phase: "workload".into(),
                });
            }
            let result = core.poll();
            if matches!(result, Observed::Verified(..)) {
                return result;
            }
            drop(core);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn complete_foreground(
        &self,
        run: &mut crate::run::ForegroundRun,
        exit: Exit,
    ) -> Result<(), RunError> {
        let core = lock(&self.core);
        if core.exit != Some(exit) {
            return Err(RunError::broken("unverified foreground process exit"));
        }
        run.record_mut().completed(
            &self.ticket,
            crate::outcome::RequiredObligation::ProcessExit(exit.code),
        )
    }

    pub fn complete(&self, run: &mut Run<'_>, exit: Exit) -> Result<(), RunError> {
        let core = lock(&self.core);
        if core.exit != Some(exit) {
            return Err(RunError::broken("unverified process exit"));
        }
        run.completed(
            &self.ticket,
            crate::outcome::RequiredObligation::ProcessExit(exit.code),
        )
    }
    pub fn forward(&mut self, signal: i32) -> Observed<SignalDelivered> {
        lock(&self.core).signal(signal)
    }
    /// Writes operator input to a `Pty` workload's terminal. Other terminals
    /// have no input channel of their own.
    pub fn write_input(&self, bytes: &[u8]) -> io::Result<()> {
        lock(&self.core).write_input(bytes)
    }
    /// Writes one bounded message to a `Terminal::Pipe` child's stdin. It
    /// never blocks past `deadline`; a child that stopped reading is a
    /// delivery failure, not a stall.
    pub fn write_stdin(&self, bytes: &[u8], deadline: Instant) -> io::Result<()> {
        lock(&self.core).write_stdin(bytes, deadline)
    }
    pub fn stop(&mut self, cx: &CleanupContext) -> StopReport {
        lock(&self.core).stop(cx)
    }
}
impl ProcessCore {
    fn abandon_capture(&mut self, reason: &str) {
        self.capture = CaptureState::Truncated(reason.to_owned());
        self.stdout = None;
        self.stderr = None;
        self.capture_eof_sent = true;
        // Both old pipes are closed. Reset terminal framing at this generation
        // boundary while retaining merged secret lookahead for its successor.
        // Real writer/provenance failure remains fatal; truncation alone does
        // not override the host's declared restart policy.
        if let Some(output) = &self.canonical_output {
            if output.abandon_generation().is_err() {
                output.fail();
            }
        }
    }
    /// One sanitize-then-redact over both streams in arrival order, before any
    /// sink: per-stream work could reconstruct a secret split across streams
    /// wherever they are joined (a log reader, a tail, a terminal), and a
    /// terminal control inside a known secret could hide it from redaction
    /// while a front end still shows the value (K12).
    fn sink(&mut self, index: usize, bytes: &[u8], eof: bool) {
        if let Some(output) = &self.canonical_output {
            if output.feed(index, bytes, eof).is_err() {
                output.fail();
                self.capture = CaptureState::Truncated("canonical capture failed".into());
            }
            return;
        }
        // Nothing can be redacted into a form a reader cannot join across, so
        // the capture stops instead of emitting a joinable marker (K12). The
        // workload itself is untouched: this is capture state only.
        if self.pipeline.redactor.blocked() {
            if self.capture == CaptureState::Complete {
                self.capture =
                    CaptureState::Truncated("no reader-safe redaction replacement".into());
            }
            return;
        }
        let tagged: Vec<(u8, u8)> = bytes.iter().map(|byte| (index as u8, *byte)).collect();
        let clean = self.pipeline.push_tagged(&tagged, eof);
        // A stream's own sink is a projection, so it is written from its own
        // redacted projection and never from a filtered merged slice: removing
        // the other stream's bytes there could join halves of a secret the
        // merged redaction could not see as one value.
        let parts = self.pipeline.project(&clean, eof);
        for (stream, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            if let Some(log) = self.logs[stream].as_mut() {
                if log.append(part).is_err() {
                    log.mark_truncated();
                    self.capture = CaptureState::Truncated("log write failed".into());
                    self.logs[stream] = None;
                }
            }
            let queue = &mut self.streams[stream];
            queue.extend(part.iter().copied());
            while queue.len() > self.queue_max {
                queue.pop_front();
            }
        }
        if clean.is_empty() {
            return;
        }
        self.output.extend(clean.iter().map(|(_, byte)| *byte));
        while self.output.len() > self.queue_max {
            self.output.pop_front();
        }
        self.tail.extend(clean.into_iter().map(|(_, byte)| byte));
        while self.tail.len() > self.tail_max {
            self.tail.pop_front();
        }
    }
    fn drain(&mut self) {
        for index in 0..2 {
            let fd = if index == 0 {
                self.stdout.take()
            } else {
                self.stderr.take()
            };
            let Some(handle) = fd else {
                continue;
            };
            let mut buf = [0u8; 8192];
            let mut eof = false;
            // Bound each turn even if a child writes continuously.
            for _ in 0..16 {
                // SAFETY: live nonblocking fd and writable initialized stack buffer.
                let count =
                    unsafe { libc::read(handle.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if count == 0 {
                    eof = true;
                    break;
                }
                if count < 0 {
                    let error = io::Error::last_os_error();
                    if self.pty && error.raw_os_error() == Some(libc::EIO) {
                        eof = true;
                    } else if !matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) {
                        self.capture = CaptureState::Truncated("capture read failed".into());
                        if let Some(output) = &self.canonical_output {
                            output.fail();
                        }
                        eof = true;
                    }
                    break;
                }
                if let Some(registration) = self.framed(index) {
                    let bytes = registration
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(&buf[..count as usize], false);
                    self.sink(index, &bytes, false);
                } else {
                    self.sink(index, &buf[..count as usize], false);
                }
            }
            if eof {
                if let Some(registration) = self.framed(index) {
                    let bytes = registration
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push(&[], true);
                    self.sink(index, &bytes, false);
                }
            }
            if !eof {
                if index == 0 {
                    self.stdout = Some(handle);
                } else {
                    self.stderr = Some(handle);
                }
            }
        }
        // One stream closing cannot flush a prefix that the other stream may
        // finish. Only the merged stream's EOF flushes the bounded redactor.
        if self.stdout.is_none() && self.stderr.is_none() && !self.capture_eof_sent {
            self.capture_eof_sent = true;
            self.sink(0, &[], true);
        }
    }
    /// The registration stream when `index` is the stream that carries it.
    fn framed(
        &self,
        index: usize,
    ) -> Option<Arc<Mutex<crate::remote::stream::RegistrationStream>>> {
        self.registration
            .as_ref()
            .filter(|registration| {
                registration
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .stream()
                    == index
            })
            .cloned()
    }
    fn poll(&mut self) -> Observed<Exit> {
        self.drain();
        if let Some(exit) = self.exit {
            return Observed::Verified(exit, evidence());
        }
        match spawn::peek(self.receipt.identity.pid) {
            Ok(Some(exit)) => {
                self.exit = Some(exit);
                // The foreground lease ends with the workload, not with the run:
                // the owner gets its terminal straight back after each child, so
                // a following sequential spawn starts from the foreground. The
                // anchor, the group and the settle are not asked.
                self.terminal.take();
                Observed::Verified(exit, evidence())
            }
            Ok(None) => Observed::Unknown(UnknownReason::Inconsistent("leader running".into())),
            Err(e) => Observed::Unknown(unknown(e)),
        }
    }
    fn write_stdin(&self, mut bytes: &[u8], deadline: Instant) -> io::Result<()> {
        let Some(stdin) = &self.stdin else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "the child has no owner-written stdin",
            ));
        };
        while !bytes.is_empty() {
            // SAFETY: live nonblocking pipe write end and an initialized slice.
            let count =
                unsafe { libc::write(stdin.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
            if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "the child did not read its stdin within the bound",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                return Err(error);
            }
            bytes = &bytes[count as usize..];
        }
        Ok(())
    }
    fn write_input(&self, mut bytes: &[u8]) -> io::Result<()> {
        let master = match (&self.stdout, self.pty) {
            (Some(master), true) => master,
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "workload has no terminal input",
                ))
            }
        };
        let deadline = Instant::now() + Duration::from_secs(1);
        while !bytes.is_empty() {
            // SAFETY: live nonblocking PTY master and an initialized slice.
            let count =
                unsafe { libc::write(master.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
            if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) && Instant::now() < deadline
                {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                return Err(error);
            }
            bytes = &bytes[count as usize..];
        }
        Ok(())
    }
    fn signal(&mut self, signal: i32) -> Observed<SignalDelivered> {
        if self.reaped {
            return Observed::Unknown(unknown("leader already reaped"));
        }
        // The unreaped direct child pins both leader pid and pgid. No numeric
        // signal is issued after that evidence is relinquished.
        if let Some(anchor) = &self.anchor {
            return anchor::signal(&self.receipt, anchor, signal);
        }
        platform::owner_signal(&self.receipt, signal)
    }
    pub(crate) fn stop(&mut self, cx: &CleanupContext) -> StopReport {
        self.stop_escalating(cx, &mut || false)
    }
    fn stop_escalating(
        &mut self,
        cx: &CleanupContext,
        escalate: &mut impl FnMut() -> bool,
    ) -> StopReport {
        // The stop takes its own window, counted from the declared grace. A
        // caller's cleanup context can be minted when a long-lived owner
        // starts, so by the time its workload needs stopping the budget may be
        // spent; a spent budget says nothing about whether the group closed and
        // never stands in for observing it (effigy#357).
        let window = Duration::from_millis(self.receipt.grace_millis).saturating_add(STOP_MARGIN);
        let stopped = cx.window(window);
        let mut signals = Vec::new();
        let mut residue = Vec::new();
        if !self.reaped {
            for signal in [libc::SIGTERM, libc::SIGCONT] {
                match self.signal(signal) {
                    Observed::Verified(sent, _) => signals.push(sent),
                    Observed::Unknown(r) => residue.push(r),
                    _ => {}
                }
            }
        }
        let grace = Instant::now()
            .checked_add(Duration::from_millis(self.receipt.grace_millis))
            .unwrap_or_else(Instant::now);
        let mut killed = false;
        let group = loop {
            self.poll();
            let state = anchor::closed(
                &self.receipt,
                self.anchor.as_ref(),
                self.exit.map(|_| &self.receipt.identity),
            );
            if matches!(state, Observed::Verified(..) | Observed::Absent(_)) {
                break state;
            }
            // Escalation precedes the deadline: a group that is still busy at
            // the end of the window is killed before the observation gives up.
            if (Instant::now() >= grace || crate::signal::escalated() || escalate())
                && (!killed || self.anchor.is_some())
                && !self.reaped
            {
                match self.signal(libc::SIGKILL) {
                    Observed::Verified(sent, _) => signals.push(sent),
                    Observed::Unknown(r) => residue.push(r),
                    _ => {}
                }
                killed = true;
            }
            if stopped.operation().check("group closure").is_err() {
                break Observed::Unknown(timed_out(format!(
                    "group closure of process group {} after {}s",
                    self.receipt.identity.pid,
                    window.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        while self.stdout.is_some() || self.stderr.is_some() {
            self.drain();
            if stopped.operation().check("capture drain").is_err() {
                self.abandon_capture("drain deadline");
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let group = if matches!(group, Observed::Verified(..) | Observed::Absent(_)) {
            anchor::stop(&self.receipt, self.anchor.as_ref(), &stopped)
        } else {
            group
        };
        if matches!(group, Observed::Verified(..) | Observed::Absent(_)) && !self.reaped {
            // Retain the direct child through group closure and stream drain.
            loop {
                match spawn::reap(self.receipt.identity.pid) {
                    Ok(Some(exit)) => {
                        self.exit = Some(exit);
                        self.reaped = true;
                        self.terminal.take();
                        break;
                    }
                    Err(error) => {
                        residue.push(unknown(error));
                        break;
                    }
                    Ok(None) => {}
                }
                if stopped.operation().check("leader reap").is_err() {
                    residue.push(timed_out(format!(
                        "leader {} reap after {}s",
                        self.receipt.identity.pid,
                        window.as_secs()
                    )));
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if let Observed::Unknown(reason) = &group {
            residue.push(reason.clone());
        } else if self.reaped {
            residue.clear();
        }
        StopReport {
            leader: self
                .exit
                .map(|e| Observed::Verified(e, evidence()))
                .unwrap_or_else(|| Observed::Unknown(unknown("leader not closed"))),
            group,
            capture: match &self.capture {
                CaptureState::Truncated(_) => self.capture.clone(),
                CaptureState::Complete => self
                    .canonical_output
                    .as_ref()
                    .map_or(CaptureState::Complete, CanonicalOutput::capture_state),
            },
            signals,
            residue,
        }
    }
}
/// The memory backup independently validates the complete pre-go receipt.
/// Only a ready, tagged anchor and the exact run/sequence nonce can be acked.
pub(crate) fn verify_foreground_receipt(
    effect: &Effect,
    run: crate::id::RunId,
) -> Result<(), RunError> {
    let record: StoredProcessRecord = serde_json::from_value(
        effect
            .receipt
            .clone()
            .ok_or_else(|| RunError::broken("missing foreground receipt"))?,
    )?;
    let record = record.into_record();
    if record.process.nonce != format!("{}:{}", run, effect.sequence)
        || record.process.pgid != record.process.identity.pid
        || platform::generation(&record.process.identity)
            .verified()
            .is_none()
        || !record
            .anchor
            .as_ref()
            .is_some_and(|a| anchor::verified(&record.process, a))
    {
        return Err(RunError::broken(
            "foreground receipt identity or anchor unavailable",
        ));
    }
    Ok(())
}

/// The current operation's bounded, already redacted diagnostic. It is not a
/// log stream or historical record, and does not expose the process core.
pub(crate) fn diagnostic_tail(core: &Arc<Mutex<ProcessCore>>) -> String {
    let core = lock(core);
    if let Some(output) = &core.canonical_output {
        let bytes = output.tail();
        let mut start = bytes.len().saturating_sub(4096);
        while start < bytes.len() && (0x80..=0xbf).contains(&bytes[start]) {
            start += 1;
        }
        return std::str::from_utf8(&bytes[start..])
            .map(str::to_owned)
            .unwrap_or_default();
    }
    let stderr = &core.streams[1];
    let bytes = stderr
        .iter()
        .skip(stderr.len().saturating_sub(4096))
        .copied()
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

pub(crate) fn settle_effect(
    effect: &Effect,
    live: Option<Arc<Mutex<ProcessCore>>>,
    cx: &CleanupContext,
    mut escalate: impl FnMut() -> bool,
) -> (EffectState, Option<String>) {
    let receipt = match effect
        .receipt
        .as_ref()
        .map(|v| serde_json::from_value::<StoredProcessRecord>(v.clone()))
    {
        Some(Ok(r)) => r.into_record(),
        None if matches!(effect.state, EffectState::Unknown | EffectState::Residue) => {
            return (
                EffectState::Residue,
                Some("unknown process custody has no receipt".into()),
            );
        }
        None => return (EffectState::Released, None), // no receipt: barrier never released
        _ => return (EffectState::Residue, Some("invalid custody receipt".into())),
    };
    let ProcessRecord {
        process: receipt,
        anchor,
    } = receipt;
    if receipt.pgid != receipt.identity.pid || receipt.pgid == 0 || receipt.pgid > i32::MAX as u32 {
        return (EffectState::Residue, Some("invalid group identity".into()));
    }
    let observed = if let Some(core) = live {
        let report = lock(&core).stop_escalating(cx, &mut escalate);
        if !report.residue.is_empty() {
            return (
                EffectState::Residue,
                Some(
                    report
                        .residue
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("; "),
                ),
            );
        }
        report.group
    } else if let Some(anchor) = &anchor {
        anchor::recover(&receipt, anchor, cx, &mut escalate)
    } else {
        platform::recover(&receipt, cx, &mut escalate)
    };
    match observed {
        Observed::Verified(..) | Observed::Absent(_) => (EffectState::Released, None),
        Observed::Unknown(reason) => (EffectState::Residue, Some(format!("{reason}"))),
    }
}

#[cfg(all(test, feature = "fault-injection"))]
mod tests;
