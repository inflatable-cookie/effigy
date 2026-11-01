//! The guest-to-host authority channel (§8.3, §8.11): lane A.
//!
//! A container-mode guest has no store, gate or run. It sends typed requests
//! over one authenticated, per-session channel; the host authenticates a
//! capability bound to its own records, re-resolves the request, and reaches
//! the same authority a host client reaches. This module owns the immutable v1
//! wire, the TLS server pin check, hello/version/digest negotiation,
//! authentication and enrolment, the canonical request digest, the replay
//! ordering and the typed `observe`/`invoke`/`attach`/`cancel`/`detach` entry
//! points. It owns no resolution, no run and no second admission authority:
//! resolution, admission and observation belong to the owning host engines
//! through [`Resolver`], [`Admission`] and [`Observer`], and every replay,
//! admission and attachment fact comes from the kernel's own records
//! (`run::reserve`, `run::lookup`, `Run::begin_admitted`, `run::Attachment`)
//! rather than from anything this module invents.
//!
//! Four rules order the host side. Nothing crosses the connection before the
//! pinned server identity and the protocol version are settled, so a version
//! or identity mismatch never discloses a credential. No request is answered
//! before authentication. The session fence and the grant are checked before
//! any effect, and the reservation is written before the engine is asked to
//! admit, so a retry reads what the host recorded instead of admitting twice.
//! A record the host cannot resolve is `unknown`, which grants nothing and is
//! never readmitted.

use crate::ctx::{EffectContext, ObserveContext, Placement};
use crate::id::{Digest, Generation, ResolvedInvocation, RunId};
use crate::observe::Observed;
use crate::root::StateRoot;
use crate::run::{
    self, AdmissionTicket, Attachment, AttachmentId, Client, RecordedOutcome, Recovered, Release,
    RequestAction, RequestId, Reservation, RunError, Session, UnknownAdmission,
};
use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::io;

/// The wire schema every connection declares in its `hello` frame.
pub const SCHEMA: &str = "effigy.workspace-channel.v1";
/// The one protocol version v1 selects. There is no downgrade (§8.11).
pub const PROTOCOL_V1: u32 = 1;
/// Largest frame body: the four-byte length prefix is checked against this
/// before the body is allocated or decoded.
pub const MAX_FRAME_BYTES: usize = 1 << 20;
/// Largest decoded binary stream payload; stream payloads travel as base64.
pub const MAX_STREAM_PAYLOAD: usize = 32 << 10;
/// Largest argument array a request body may carry.
pub const MAX_ARGUMENTS: usize = 256;
/// Largest single argument, path or selector a request body may carry.
pub const MAX_PATH_BYTES: usize = 4096;
/// Largest request body after canonical re-encoding.
pub const MAX_BODY_BYTES: usize = 64 << 10;
/// Stream data frames a receiver may leave unacknowledged.
pub const MAX_UNACKED_STREAM_FRAMES: usize = 16;
/// Queued decoded stream bytes before the sender backpressures.
pub const MAX_QUEUED_STREAM_BYTES: usize = 512 << 10;
/// The stream id the attached run's terminal travels on. Terminal output,
/// terminal input and resize all name it, so one attachment has one terminal
/// and one inbound sequence (052 §8.11).
pub const TERMINAL_STREAM: u32 = 0;
/// The largest cursor an attach request may name, in a stream's own units.
pub const MAX_STREAM_CURSOR: u64 = 1 << 40;
/// Per-stream cursors one attach request may name: the connection carries at
/// most nine kinds, and a source publishes bounded ids.
pub const MAX_PER_STREAM_CURSORS: usize = 16;
/// Longest diagnostic a refusal carries: a sentence, never a transcript.
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;

/// A refusal code: the stable codes §8.11 names, which a client switches on.
/// The diagnostic is for the person at the terminal and never carries a
/// credential, a frame or a host path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefusalCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    StaleContext,
    IdentityMismatch,
    VersionMismatch,
    HeldBy,
    Unavailable,
    Unknown,
    IdConflict,
}
impl RefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::StaleContext => "stale_context",
            Self::IdentityMismatch => "identity_mismatch",
            Self::VersionMismatch => "version_mismatch",
            Self::HeldBy => "held_by",
            Self::Unavailable => "unavailable",
            Self::Unknown => "unknown",
            Self::IdConflict => "id_conflict",
        }
    }
}
impl fmt::Display for RefusalCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A stable code and a safe diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Refusal {
    pub code: RefusalCode,
    pub diagnostic: String,
}
impl Refusal {
    pub fn new(code: RefusalCode, diagnostic: impl AsRef<str>) -> Self {
        let mut text = String::new();
        for ch in diagnostic.as_ref().chars() {
            if ch.is_control() {
                continue;
            }
            if text.len() + ch.len_utf8() > MAX_DIAGNOSTIC_BYTES {
                break;
            }
            text.push(ch);
        }
        Self {
            code,
            diagnostic: text,
        }
    }
    pub fn invalid(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::InvalidRequest, diagnostic)
    }
    pub fn forbidden(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::Forbidden, diagnostic)
    }
    pub fn unknown(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::Unknown, diagnostic)
    }
    pub fn unavailable(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::Unavailable, diagnostic)
    }
    pub fn stale(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::StaleContext, diagnostic)
    }
    pub fn identity(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::IdentityMismatch, diagnostic)
    }
    pub fn version(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::VersionMismatch, diagnostic)
    }
    pub fn unauthenticated(diagnostic: impl AsRef<str>) -> Self {
        Self::new(RefusalCode::Unauthenticated, diagnostic)
    }
}
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.diagnostic)
    }
}

/// An unguessable 256-bit secret: a session capability or an enrolment
/// credential. `Debug` and `Display` never print the bytes, so it cannot reach
/// stdout, a JSON envelope or a log through a formatted value, and a refusal
/// never echoes the frame that carried it.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 32]);
impl Secret {
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
    /// Length-independent comparison, so a wrong prefix costs what a wrong
    /// first byte costs.
    pub fn matches(&self, other: &Self) -> bool {
        let mut difference = 0u8;
        for (left, right) in self.0.iter().zip(other.0.iter()) {
            difference |= left ^ right;
        }
        difference == 0
    }
    pub fn parse(text: &str) -> Result<Self, Refusal> {
        let bytes = hex_bytes(text, 32).ok_or_else(|| {
            Refusal::invalid("a channel credential is 64 lowercase hex characters")
        })?;
        let mut secret = [0u8; 32];
        secret.copy_from_slice(&bytes);
        Ok(Self(secret))
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}
impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}
impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&hex_text(&self.0))
    }
}
impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}
fn hex_text(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(nibble_char(byte >> 4));
        text.push(nibble_char(byte & 0x0f));
    }
    text
}
fn nibble_char(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        _ => char::from(b'a' + value - 10),
    }
}
fn hex_bytes(text: &str, bytes: usize) -> Option<Vec<u8>> {
    if text.len() != bytes * 2 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes);
    for pair in text.as_bytes().chunks_exact(2) {
        let high = nibble_value(pair[0])?;
        let low = nibble_value(pair[1])?;
        out.push(high << 4 | low);
    }
    Some(out)
}
fn nibble_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// The request method. The label grants nothing: the host decides the effect
/// class of an invocation from its own policy (§8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Observe,
    Invoke,
    Attach,
    Cancel,
    Detach,
}
impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Invoke => "invoke",
            Self::Attach => "attach",
            Self::Cancel => "cancel",
            Self::Detach => "detach",
        }
    }
    /// Only an invocation can admit a run, so only it has a reservation.
    pub fn admits(self) -> bool {
        matches!(self, Self::Invoke)
    }
    pub fn mutates(self) -> bool {
        !matches!(self, Self::Observe)
    }
}

/// Where a re-resolved invocation runs. A guest names a target the host
/// already knows; it never names an executable, entry point or state root.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Host,
    Container { service: String },
}
impl Target {
    fn check(&self) -> Result<(), Refusal> {
        match self {
            Self::Host => Ok(()),
            Self::Container { service } => {
                if service.is_empty() || service.len() > 128 {
                    return Err(Refusal::invalid("a container target names a service"));
                }
                if !service
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
                {
                    return Err(Refusal::invalid(
                        "a container service name is letters, digits, dot, underscore or dash",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// The selector or typed operation name a request carries. The host resolves
/// it; the channel only bounds its shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    Task(String),
    Operation(String),
    Shell(String),
}
impl Selector {
    pub fn text(&self) -> &str {
        match self {
            Self::Task(value) | Self::Operation(value) | Self::Shell(value) => value,
        }
    }
    fn check(&self) -> Result<(), Refusal> {
        let text = self.text();
        if text.is_empty() || text.len() > MAX_PATH_BYTES {
            return Err(Refusal::invalid("a selector is non-empty and bounded"));
        }
        if text.chars().any(char::is_control) {
            return Err(Refusal::invalid("a selector carries no control characters"));
        }
        Ok(())
    }
}

/// The output rendering the client asked for. It is not a privilege: `--json`
/// is rendered through the existing `effigy.command.v1` envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMode {
    Text,
    Json,
}

/// The body of an `invoke` request. There is no resolved plan, executable path,
/// entry point or state root slot, and an unknown field refuses (§8.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationBody {
    pub selector: Selector,
    pub cwd: String,
    pub target: Target,
    pub args: Vec<String>,
    pub output: OutputMode,
}
impl InvocationBody {
    fn check(&self) -> Result<(), Refusal> {
        self.selector.check()?;
        self.target.check()?;
        check_relative_path(&self.cwd)?;
        if self.args.len() > MAX_ARGUMENTS {
            return Err(Refusal::invalid("a request carries at most 256 arguments"));
        }
        let mut total = 0usize;
        for argument in &self.args {
            total = total.saturating_add(argument.len());
            if argument.len() > MAX_PATH_BYTES {
                return Err(Refusal::invalid("an argument is at most 4096 bytes"));
            }
            if argument.contains('\0') {
                return Err(Refusal::invalid("an argument carries no NUL"));
            }
        }
        if total > MAX_BODY_BYTES {
            return Err(Refusal::invalid("a request body is at most 64 KiB"));
        }
        Ok(())
    }
}

/// The body of an `observe` request. `op` is a bounded operation name whose
/// grammar belongs to the observing engine; the channel carries it and the
/// host runs it under [`ObserveContext`], which writes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveBody {
    pub op: String,
    pub target: Option<Target>,
    /// Exact run identity for run-specific observations. Omitted on legacy reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}
impl ObserveBody {
    fn check(&self) -> Result<(), Refusal> {
        match (&*self.op, &self.run_id) {
            ("runs:status", Some(id)) if id.parse::<RunId>().is_ok() => {}
            ("runs:status", _) => {
                return Err(Refusal::invalid(
                    "runs:status requires an exact run identity",
                ))
            }
            (_, None) => {}
            (_, Some(_)) => {
                return Err(Refusal::invalid(
                    "run identity is not valid for this observation",
                ))
            }
        }
        if self.op.is_empty() || self.op.len() > 120 {
            return Err(Refusal::invalid("an observe operation name is bounded"));
        }
        if !self
            .op
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b':' | b'_' | b'-'))
        {
            return Err(Refusal::invalid(
                "an observe operation name is letters, digits, colon, underscore or dash",
            ));
        }
        if let Some(target) = &self.target {
            target.check()?;
        }
        Ok(())
    }
}

/// What an `attach` request asks to read. A cursor asks to resume; when
/// rotation removed those bytes the first read reports a gap, never complete
/// output.
///
/// Retained streams have independent logs and independent lengths, so a client
/// that consumed more of one than another reports each position in `cursors`,
/// keyed by the stream id it read. `cursor` remains the default for a
/// connection that names a single position, and the only cursor a
/// single-stream attach needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachBody {
    pub run: RunId,
    pub streams: Vec<StreamKind>,
    pub cursor: Option<u64>,
    #[serde(default)]
    pub cursors: Option<BTreeMap<u32, u64>>,
}
impl AttachBody {
    fn check(&self) -> Result<(), Refusal> {
        if self.streams.is_empty() || self.streams.len() > 9 {
            return Err(Refusal::invalid(
                "an attach request names between one and nine streams",
            ));
        }
        let unique: BTreeSet<StreamKind> = self.streams.iter().copied().collect();
        if unique.len() != self.streams.len() {
            return Err(Refusal::invalid("an attach request names each stream once"));
        }
        if self.cursor.is_some_and(|cursor| cursor > MAX_STREAM_CURSOR) {
            return Err(Refusal::invalid("a stream cursor is bounded"));
        }
        if let Some(cursors) = &self.cursors {
            if cursors.len() > MAX_PER_STREAM_CURSORS {
                return Err(Refusal::invalid(
                    "an attach request names at most sixteen stream cursors",
                ));
            }
            if cursors.values().any(|cursor| *cursor > MAX_STREAM_CURSOR) {
                return Err(Refusal::invalid("a stream cursor is bounded"));
            }
        }
        Ok(())
    }
    /// The positions this request reports, per stream id, with its default
    /// cursor for a stream it did not name individually.
    fn positions(&self) -> Cursors {
        Cursors {
            primary: self.cursor.unwrap_or(0),
            streams: self.cursors.clone().unwrap_or_default(),
        }
    }
}

/// A `cancel` or `detach` request names the run it is about. Naming a run is
/// not authority: the grant's target scope and the kernel's own attachment
/// decide (§8.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBody {
    pub run: RunId,
}

/// A request body, typed by method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestBody {
    Observe(ObserveBody),
    Invoke(InvocationBody),
    Attach(AttachBody),
    Cancel(RunBody),
    Detach(RunBody),
}
impl RequestBody {
    pub fn method(&self) -> Method {
        match self {
            Self::Observe(_) => Method::Observe,
            Self::Invoke(_) => Method::Invoke,
            Self::Attach(_) => Method::Attach,
            Self::Cancel(_) => Method::Cancel,
            Self::Detach(_) => Method::Detach,
        }
    }
    fn check(&self) -> Result<(), Refusal> {
        match self {
            Self::Observe(body) => body.check(),
            Self::Invoke(body) => body.check(),
            Self::Attach(body) => body.check(),
            Self::Cancel(_) | Self::Detach(_) => Ok(()),
        }?;
        let body =
            serde_json::to_vec(self).map_err(|_| Refusal::invalid("a body is not encodable"))?;
        if body.len() > MAX_BODY_BYTES {
            return Err(Refusal::invalid("a request body is at most 64 KiB"));
        }
        Ok(())
    }
    pub fn target(&self) -> Option<&Target> {
        match self {
            Self::Invoke(body) => Some(&body.target),
            Self::Observe(body) => body.target.as_ref(),
            _ => None,
        }
    }
}

/// One authenticated request. Every field is the client's claim; the host
/// binds them to its grant and refuses a mismatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub id: RequestId,
    pub context_generation: Generation,
    pub environment_generation: Generation,
    pub session_run_id: RunId,
    pub container_id: String,
    pub vm_boot_id: String,
    pub body: RequestBody,
}
impl Request {
    pub fn method(&self) -> Method {
        self.body.method()
    }
    /// The canonical request digest: the typed request re-encoded, so framing,
    /// whitespace and key order cannot change it and a different body always
    /// can. Two retries of one intent share a digest; one id with two intents
    /// is an `id_conflict` (§8.11).
    pub fn digest(&self) -> Result<Digest, Refusal> {
        let canonical = serde_json::to_vec(&CanonicalRequest {
            id: self.id,
            method: self.method(),
            context_generation: self.context_generation,
            environment_generation: self.environment_generation,
            session_run_id: self.session_run_id,
            container_id: &self.container_id,
            vm_boot_id: &self.vm_boot_id,
            body: &self.body,
        })
        .map_err(|_| Refusal::invalid("a request is not encodable"))?;
        let mut material = b"effigy.workspace-channel.request\0\x01".to_vec();
        material.extend_from_slice(&canonical);
        Ok(Digest::of(&material))
    }
}
#[derive(Serialize)]
struct CanonicalRequest<'a> {
    id: RequestId,
    method: Method,
    context_generation: Generation,
    environment_generation: Generation,
    session_run_id: RunId,
    container_id: &'a str,
    vm_boot_id: &'a str,
    body: &'a RequestBody,
}
impl Serialize for RequestBody {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Observe(body) => body.serialize(serializer),
            Self::Invoke(body) => body.serialize(serializer),
            Self::Attach(body) => body.serialize(serializer),
            Self::Cancel(body) => body.serialize(serializer),
            Self::Detach(body) => body.serialize(serializer),
        }
    }
}

/// Reject an absolute or escaping path before dispatch: cwd resolves inside
/// the allowed checkout or target, never above it.
pub fn check_relative_path(path: &str) -> Result<(), Refusal> {
    if path.len() > MAX_PATH_BYTES {
        return Err(Refusal::invalid("a working directory is bounded"));
    }
    if path.starts_with('/') || path.contains(['\\', '\0']) {
        return Err(Refusal::invalid(
            "a working directory is checkout-relative, with no backslash or NUL",
        ));
    }
    if path.split('/').any(|part| part == "..") {
        return Err(Refusal::invalid(
            "a working directory does not traverse above the checkout",
        ));
    }
    Ok(())
}

/// What the host knows of its own artifact and protocol (§8.9). A client whose
/// version or digest differs has no v1 session: there is no downgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostArtifact {
    pub effigy_version: String,
    pub binary_digest: Digest,
}

/// The immutable facts a connection is negotiated against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiation {
    pub context_generation: Generation,
    pub session_run_id: RunId,
    pub artifact: HostArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub schema: String,
    pub protocol_versions: Vec<u32>,
    pub effigy_version: String,
    pub binary_digest: Digest,
    pub context_generation: Generation,
    pub session_run_id: RunId,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelloOk {
    pub protocol: u32,
    pub effigy_version: String,
}

/// The TLS identity the transport verified. The pin check compares it with the
/// identity the selected context pinned, before any credential frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerIdentity {
    pub digest: Digest,
}

/// Refuse unless the transport's verified peer identity is the pinned one. A
/// transport that cannot report a verified identity refuses: an unverified
/// connection is not a pin.
pub fn check_server_pin(pinned: &Digest, peer: Option<&ServerIdentity>) -> Result<(), Refusal> {
    let Some(peer) = peer else {
        return Err(Refusal::identity(
            "the connection did not present a verified server identity, so it cannot be the pinned host",
        ));
    };
    let mut difference = 0u8;
    for (left, right) in pinned.0.iter().zip(peer.digest.0.iter()) {
        difference |= left ^ right;
    }
    if difference != 0 {
        return Err(Refusal::identity(
            "the server identity is not the one this context pinned; re-prepare the workspace",
        ));
    }
    Ok(())
}

/// Negotiate one connection's protocol, after the pin check and before
/// anything else is written, so a version or identity mismatch refuses before
/// effects and before a credential is disclosed.
pub fn negotiate(client: &Hello, host: &Negotiation) -> Result<HelloOk, Refusal> {
    if client.schema != SCHEMA {
        return Err(Refusal::version(
            "this client speaks another channel schema; rerun the binary this workspace provisioned",
        ));
    }
    if !client.protocol_versions.contains(&PROTOCOL_V1) {
        return Err(Refusal::version(
            "this client offers no protocol version the host speaks; there is no downgrade, so update the guest binary",
        ));
    }
    if client.effigy_version != host.artifact.effigy_version
        || client.binary_digest != host.artifact.binary_digest
    {
        return Err(Refusal::version(
            "the guest binary is not the one this workspace pinned; re-prepare the workspace",
        ));
    }
    if client.context_generation != host.context_generation {
        return Err(Refusal::stale(
            "this client read an older context generation; reread the current one",
        ));
    }
    if client.session_run_id != host.session_run_id {
        return Err(Refusal::identity(
            "this client names a different workspace session; re-prepare the workspace",
        ));
    }
    Ok(HelloOk {
        protocol: PROTOCOL_V1,
        effigy_version: host.artifact.effigy_version.clone(),
    })
}

/// The host facts a request is fenced against. These are the host's own
/// records; a mismatch refuses and grants nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionFence {
    pub context_generation: Generation,
    pub environment_generation: Generation,
    pub session: Session,
    pub container_id: String,
    pub vm_boot_id: String,
}
impl SessionFence {
    /// A grant is only usable on the connection whose session facts it was
    /// issued for. The client's request claims are checked separately; this is
    /// the host's own binding, so a grant from another session, container,
    /// environment generation or VM boot can never be replayed here.
    fn verify_grant(&self, binding: &GrantBinding) -> Result<(), Refusal> {
        if binding.session != self.session {
            return Err(Refusal::identity(
                "this capability belongs to another workspace session",
            ));
        }
        if binding.environment_generation != self.environment_generation {
            return Err(Refusal::stale(
                "this capability was issued for another environment generation",
            ));
        }
        if binding.container_id != self.container_id || binding.vm_boot_id != self.vm_boot_id {
            return Err(Refusal::identity(
                "this capability was issued for another container or VM boot",
            ));
        }
        Ok(())
    }
    fn verify(&self, request: &Request) -> Result<(), Refusal> {
        if request.context_generation != self.context_generation {
            return Err(Refusal::stale(
                "this request carries an older context generation; reread the current one",
            ));
        }
        if request.environment_generation != self.environment_generation {
            return Err(Refusal::stale(
                "the environment changed generation since this credential was issued",
            ));
        }
        if request.session_run_id != self.session.run {
            return Err(Refusal::identity(
                "this request names a different workspace session",
            ));
        }
        if request.container_id != self.container_id || request.vm_boot_id != self.vm_boot_id {
            return Err(Refusal::identity(
                "this request comes from a different container or VM boot than the grant names",
            ));
        }
        Ok(())
    }
}

/// One session-bound grant. It lives in the session supervisor's memory for as
/// long as the session run (§8.3) and binds the identity it was issued to, the
/// operations it allows and the targets it may reach.
#[derive(Debug, Clone)]
pub struct Grant {
    secret: Secret,
    binding: GrantBinding,
    operations: BTreeSet<Method>,
    targets: BTreeSet<Target>,
    revoked: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantBinding {
    pub host_user: String,
    pub checkout: String,
    pub session: Session,
    pub environment_generation: Generation,
    pub container_id: String,
    pub vm_boot_id: String,
}
impl Grant {
    pub fn binding(&self) -> &GrantBinding {
        &self.binding
    }
    pub fn session(&self) -> Session {
        self.binding.session
    }
    pub fn revoked(&self) -> bool {
        self.revoked
    }
    /// An authenticated operation, checked against this grant's scope. Naming
    /// a run, a target or a session grants nothing on its own.
    pub fn authorise(&self, method: Method, target: Option<&Target>) -> Result<(), Refusal> {
        if self.revoked {
            return Err(Refusal::unauthenticated(
                "this capability was revoked when its generation ended",
            ));
        }
        if !self.operations.contains(&method) {
            return Err(Refusal::forbidden(format!(
                "this capability does not allow {}",
                method.as_str()
            )));
        }
        if let Some(target) = target {
            if !self.targets.contains(target) {
                return Err(Refusal::forbidden(
                    "this capability does not allow that target",
                ));
            }
        }
        Ok(())
    }
}

/// What the host installs for enrolment: one credential, the environment and
/// container it is bound to, and the scope the capabilities it issues get. The
/// client's `enrol` frame cannot widen any of it.
#[derive(Debug, Clone)]
pub struct Enrolment {
    pub credential: Secret,
    /// The session this credential enrols into. The host knows it; the guest's
    /// frame must agree with it, and a frame that names another session refuses.
    pub session: Session,
    pub environment_generation: Generation,
    pub container_id: String,
    pub vm_boot_id: String,
    pub host_user: String,
    pub checkout: String,
    pub scope: EnrolmentScope,
}
/// What an enrolment credential is allowed to issue. The host decides this
/// scope; the client's `enrol` frame cannot widen it.
#[derive(Debug, Clone)]
pub struct EnrolmentScope {
    pub operations: BTreeSet<Method>,
    pub targets: BTreeSet<Target>,
}
#[derive(Debug, Clone)]
struct EnrolmentGrant {
    secret: Secret,
    session: Session,
    environment_generation: Generation,
    container_id: String,
    vm_boot_id: String,
    host_user: String,
    checkout: String,
    scope: EnrolmentScope,
}

/// The session's grants. Memory only: nothing here is persisted, and the
/// session run's settle drops it, ending every grant it issued.
#[derive(Debug, Default)]
pub struct GrantBook {
    grants: Vec<Grant>,
    enrolment: Option<EnrolmentGrant>,
}
impl GrantBook {
    pub fn new() -> Self {
        Self::default()
    }
    /// Issue a capability for one client session. The caller decides the
    /// binding and scope; the channel never widens them.
    pub fn issue(
        &mut self,
        secret: Secret,
        binding: GrantBinding,
        operations: BTreeSet<Method>,
        targets: BTreeSet<Target>,
    ) -> Secret {
        self.grants.push(Grant {
            secret: secret.clone(),
            binding,
            operations,
            targets,
            revoked: false,
        });
        secret
    }
    /// Install the separate, environment-generation and container-bound
    /// enrolment credential. It can issue one capability and nothing else.
    pub fn enrollable(&mut self, enrolment: Enrolment) {
        self.enrolment = Some(EnrolmentGrant {
            secret: enrolment.credential,
            session: enrolment.session,
            environment_generation: enrolment.environment_generation,
            container_id: enrolment.container_id,
            vm_boot_id: enrolment.vm_boot_id,
            host_user: enrolment.host_user,
            checkout: enrolment.checkout,
            scope: enrolment.scope,
        });
    }
    pub fn authenticate(&self, presented: &Secret) -> Result<&Grant, Refusal> {
        let grant = self
            .grants
            .iter()
            .find(|grant| grant.secret.matches(presented))
            .ok_or_else(|| {
                Refusal::unauthenticated("this capability was not issued by this session")
            })?;
        if grant.revoked {
            return Err(Refusal::unauthenticated(
                "this capability was revoked when its generation ended",
            ));
        }
        Ok(grant)
    }
    /// Enrolment issues a capability for the client's own secret, bound to the
    /// host's enrolment scope. It cannot execute an operation itself.
    fn enrol(&mut self, presented: &Secret, request: &EnrolRequest) -> Result<Grant, Refusal> {
        let enrolment = self.enrolment.as_ref().ok_or_else(|| {
            Refusal::forbidden("this host issued no enrolment credential for the session")
        })?;
        if !enrolment.secret.matches(presented) {
            return Err(Refusal::unauthenticated(
                "this enrolment credential was not issued by this workspace",
            ));
        }
        if enrolment.environment_generation != request.environment_generation {
            return Err(Refusal::stale(
                "the enrolment credential is for another environment generation; re-prepare the workspace",
            ));
        }
        if enrolment.container_id != request.container_id
            || enrolment.vm_boot_id != request.vm_boot_id
        {
            return Err(Refusal::identity(
                "this enrolment credential is bound to another container or VM boot",
            ));
        }
        // The session comes from the host's own enrolment record. A frame that
        // names another run or generation refuses, so a guest cannot enrol a
        // capability into a session it was not given (§8.11).
        if request.session_run_id != enrolment.session.run
            || request.session_generation != enrolment.session.generation
        {
            return Err(Refusal::identity(
                "this enrolment names a different workspace session than the credential was issued for",
            ));
        }
        let binding = GrantBinding {
            host_user: enrolment.host_user.clone(),
            checkout: enrolment.checkout.clone(),
            session: enrolment.session,
            environment_generation: enrolment.environment_generation,
            container_id: enrolment.container_id.clone(),
            vm_boot_id: enrolment.vm_boot_id.clone(),
        };
        let grant = Grant {
            secret: Secret::new(request.capability),
            binding,
            operations: enrolment.scope.operations.clone(),
            targets: enrolment.scope.targets.clone(),
            revoked: false,
        };
        self.grants.push(grant.clone());
        Ok(grant)
    }
    /// End every grant bound to a session run at or below a generation, and
    /// end the enrolment credential that could mint a new one for it. A
    /// capability from a dead generation is worth nothing, and re-enrolment
    /// cannot revive it (§8.11: the credential is revoked on generation change).
    pub fn revoke_generation(&mut self, session_run_id: RunId, generation: Generation) {
        for grant in &mut self.grants {
            if grant.binding.session.run == session_run_id
                && grant.binding.session.generation <= generation
            {
                grant.revoked = true;
            }
        }
        if self.enrolment.as_ref().is_some_and(|enrolment| {
            enrolment.session.run == session_run_id && enrolment.session.generation <= generation
        }) {
            self.enrolment = None;
        }
    }
    /// Whether this book can still enrol a capability. A revoked generation
    /// leaves nothing to enrol with.
    pub fn enrolable(&self) -> bool {
        self.enrolment.is_some()
    }
    pub fn revoke_all(&mut self) {
        for grant in &mut self.grants {
            grant.revoked = true;
        }
        self.enrolment = None;
    }
}

/// An `enrol` frame: the enrolment credential plus the capability secret the
/// client will present from now on. It carries no operation, so enrolment
/// never executes one, and the host's enrolment scope decides what the issued
/// capability may do.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrolRequest {
    pub credential: Secret,
    pub capability: [u8; 32],
    pub session_run_id: RunId,
    pub session_generation: Generation,
    pub environment_generation: Generation,
    pub container_id: String,
    pub vm_boot_id: String,
}
impl fmt::Debug for EnrolRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EnrolRequest")
            .field("credential", &"<redacted>")
            .field("capability", &"<redacted>")
            .field("session_run_id", &self.session_run_id)
            .field("session_generation", &self.session_generation)
            .field("environment_generation", &self.environment_generation)
            .finish_non_exhaustive()
    }
}

/// A stream kind. Resize carries positive rows and columns; a gap names what
/// was missing and is never presented as complete output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    Event,
    Stdout,
    Stderr,
    TerminalOutput,
    TerminalInput,
    Resize,
    Ack,
    End,
    Gap,
}
impl StreamKind {
    pub fn is_data(self) -> bool {
        matches!(
            self,
            Self::Event | Self::Stdout | Self::Stderr | Self::TerminalOutput
        )
    }
}

/// A stream frame. `seq` is monotonic per stream id and shared by every kind
/// on it, so stdout then stderr on one stream is `0, 1` (052 §8.11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamFrame {
    pub id: RequestId,
    pub stream_id: u32,
    pub seq: u64,
    pub kind: StreamKind,
    /// Base64 of at most [`MAX_STREAM_PAYLOAD`] decoded bytes.
    pub payload: Option<String>,
    pub rows: Option<u16>,
    pub columns: Option<u16>,
}

/// The range one `gap` frame reports: `from` was rotated away and output
/// resumes at `to`. A gap is never presented as complete output, and a client
/// obtains current status or logs from the host for the missing range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamGap {
    pub from: u64,
    pub to: u64,
}

/// One terminal geometry a `resize` frame carries. The owner keeps the newest
/// size; the channel holds no queue of earlier sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalSize {
    pub rows: u16,
    pub columns: u16,
}

impl StreamFrame {
    /// A bounded gap report on `stream_id`. Its sequence is the first position
    /// the log can no longer give, so the data that resumes at `to` keeps the
    /// stream's sequence strictly increasing, and its payload is the range.
    pub fn gap(id: RequestId, stream_id: u32, gap: StreamGap) -> Result<Self, Refusal> {
        let bytes = serde_json::to_vec(&gap)
            .map_err(|_| Refusal::invalid("a stream gap is not encodable"))?;
        Ok(Self {
            id,
            stream_id,
            seq: gap.from,
            kind: StreamKind::Gap,
            payload: Some(encode_stream_payload(&bytes)),
            rows: None,
            columns: None,
        })
    }
    /// The missing range a `gap` frame reports, when it carries one.
    pub fn gap_range(&self) -> Option<StreamGap> {
        if self.kind != StreamKind::Gap {
            return None;
        }
        let bytes = decode_stream_payload(self.payload.as_deref()?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }
    fn check(&self) -> Result<(), Refusal> {
        if self.kind == StreamKind::Resize {
            match (self.rows, self.columns) {
                (Some(rows), Some(columns)) if rows > 0 && columns > 0 => {}
                _ => {
                    return Err(Refusal::invalid(
                        "a resize frame carries positive rows and columns",
                    ))
                }
            }
        }
        if let Some(payload) = &self.payload {
            decode_stream_payload(payload)?;
        }
        Ok(())
    }
}

/// Encode stream bytes as base64 for a frame payload.
pub fn encode_stream_payload(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut block = [0u8; 3];
        block[..chunk.len()].copy_from_slice(chunk);
        let value = (block[0] as u32) << 16 | (block[1] as u32) << 8 | block[2] as u32;
        for index in 0..4 {
            if index <= chunk.len() {
                let shift = 18 - index * 6;
                out.push(ALPHABET[((value >> shift) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A bounded base64 decoder. A payload that is not base64, or decodes larger
/// than [`MAX_STREAM_PAYLOAD`], refuses before the bytes are used.
pub fn decode_stream_payload(payload: &str) -> Result<Vec<u8>, Refusal> {
    let max_encoded = MAX_STREAM_PAYLOAD.div_ceil(3) * 4;
    if payload.len() > max_encoded {
        return Err(Refusal::invalid(
            "a stream payload is at most 32 KiB decoded",
        ));
    }
    if !payload.len().is_multiple_of(4) {
        return Err(Refusal::invalid("a stream payload is base64"));
    }
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(payload.len() / 4 * 3);
    let mut padding = 0usize;
    for (index, chunk) in bytes.chunks(4).enumerate() {
        let last = (index + 1) * 4 == bytes.len();
        let mut block = [0u8; 4];
        for (offset, byte) in chunk.iter().enumerate() {
            block[offset] = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' if offset >= 2 && last => {
                    padding += 1;
                    0
                }
                _ => return Err(Refusal::invalid("a stream payload is base64")),
            };
            if padding > 0 && *byte != b'=' {
                return Err(Refusal::invalid("a stream payload is base64"));
            }
        }
        let value = (block[0] as u32) << 18
            | (block[1] as u32) << 12
            | (block[2] as u32) << 6
            | block[3] as u32;
        out.push((value >> 16) as u8);
        if padding < 2 {
            out.push((value >> 8) as u8);
        }
        if padding < 1 {
            out.push(value as u8);
        }
        if out.len() > MAX_STREAM_PAYLOAD {
            return Err(Refusal::invalid(
                "a stream payload is at most 32 KiB decoded",
            ));
        }
    }
    Ok(out)
}

/// What the sender may queue. Data frames are bounded by the receiver's
/// acknowledgement; control frames are not, so cancellation stays serviceable
/// under backpressure (§8.11).
#[derive(Debug, Default)]
pub struct StreamWindow {
    unacked: usize,
    queued: usize,
    /// Every published frame this connection has not been acknowledged for, in
    /// publication order, with its stream and size. Streams interleave on one
    /// connection, so a frame is released by its own stream's acknowledgement
    /// and never waits for another stream's frames to be consumed first.
    open: VecDeque<OpenFrame>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OpenFrame {
    stream_id: u32,
    seq: u64,
    size: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Queue {
    Admitted,
    Backpressure,
}
impl StreamWindow {
    pub fn new() -> Self {
        Self::default()
    }
    /// Whether one more data frame of `decoded` bytes fits. A frame becomes
    /// open only when [`StreamWindow::track`] records it.
    pub fn offer(&self, decoded: usize) -> Queue {
        if decoded > MAX_STREAM_PAYLOAD
            || self.unacked + 1 > MAX_UNACKED_STREAM_FRAMES
            || self.queued.saturating_add(decoded) > MAX_QUEUED_STREAM_BYTES
        {
            return Queue::Backpressure;
        }
        Queue::Admitted
    }
    /// The receiver consumed up to `seq` on `stream_id`, which names an open
    /// data frame this connection published. Every frame of that stream at or
    /// below `seq` is released wherever it sits between other streams' frames.
    /// A sequence that is not open — a duplicate, a stale one, a control frame
    /// or a frame another connection published — frees nothing and reports
    /// `false`, so an invalid acknowledgement has no effect on the window.
    pub fn acknowledge(&mut self, stream_id: u32, seq: u64) -> bool {
        if !self
            .open
            .iter()
            .any(|frame| frame.stream_id == stream_id && frame.seq == seq)
        {
            return false;
        }
        let mut frames = 0usize;
        let mut bytes = 0usize;
        self.open.retain(|frame| {
            if frame.stream_id == stream_id && frame.seq <= seq {
                frames += 1;
                bytes = bytes.saturating_add(frame.size);
                false
            } else {
                true
            }
        });
        self.unacked = self.unacked.saturating_sub(frames);
        self.queued = self.queued.saturating_sub(bytes);
        true
    }
    /// Record one published data frame against the window.
    fn track(&mut self, stream_id: u32, seq: u64, size: usize) {
        self.open.push_back(OpenFrame {
            stream_id,
            seq,
            size,
        });
        self.unacked += 1;
        self.queued = self.queued.saturating_add(size);
    }
    pub fn unacked(&self) -> usize {
        self.unacked
    }
    pub fn queued(&self) -> usize {
        self.queued
    }
}
/// The connection's outbound stream queue: data frames wait in the window,
/// control frames never do.
#[derive(Debug, Default)]
pub struct Outbound {
    window: StreamWindow,
    data: Vec<StreamFrame>,
    /// The next position each stream id will publish. Numbering is per stream
    /// id and shared by every kind on it (§8.11), and its unit is whatever the
    /// source reads in: a retained stream's absolute byte offset, or a plain
    /// frame ordinal for a source that keeps no position.
    next_seq: BTreeMap<u32, u64>,
}
impl Outbound {
    pub fn new() -> Self {
        Self::default()
    }
    /// Queue one captured chunk as data frames, stopping at the window. The
    /// bytes this did not send are returned so the reader can ask again later.
    pub fn push_chunk<'a>(
        &mut self,
        id: RequestId,
        stream_id: u32,
        kind: StreamKind,
        bytes: &'a [u8],
    ) -> &'a [u8] {
        self.push(id, stream_id, kind, None, bytes)
            .expect("a positional chunk is the only one that can move backwards")
    }
    /// Queue one chunk the retained reader holds at absolute position `from`,
    /// so the client's resume cursor, the frame sequences and the retained log
    /// are one coordinate system across reconnects and chunk boundaries.
    ///
    /// A jump forward — bytes the log rotated away — is reported as one
    /// `gap` frame that names where it resumes, so a receiver enforcing a
    /// strictly increasing sequence sees the control frame and the data that
    /// follows as distinct positions. A position behind what this connection
    /// already published is a source defect and refuses.
    pub fn push_retained<'a>(
        &mut self,
        id: RequestId,
        stream_id: u32,
        kind: StreamKind,
        from: u64,
        bytes: &'a [u8],
    ) -> Result<&'a [u8], Refusal> {
        self.push(id, stream_id, kind, Some(from), bytes)
    }
    fn push<'a>(
        &mut self,
        id: RequestId,
        stream_id: u32,
        kind: StreamKind,
        from: Option<u64>,
        bytes: &'a [u8],
    ) -> Result<&'a [u8], Refusal> {
        let next = *self.next_seq.entry(stream_id).or_insert(0);
        if let Some(from) = from {
            if from < next {
                return Err(Refusal::unknown(
                    "the retained stream position moved behind what this connection published",
                ));
            }
            if from > next {
                // The bounded range the log no longer holds. Its sequence is
                // the first missing position, so the data that resumes at
                // `from` keeps the stream's sequence strictly increasing.
                self.data.push(StreamFrame::gap(
                    id,
                    stream_id,
                    StreamGap {
                        from: next,
                        to: from,
                    },
                )?);
                self.next_seq.insert(stream_id, from);
            }
        }
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + MAX_STREAM_PAYLOAD).min(bytes.len());
            if self.window.offer(end - offset) == Queue::Backpressure {
                break;
            }
            let seq = *self.next_seq.entry(stream_id).or_insert(0);
            self.window.track(stream_id, seq, end - offset);
            self.data.push(StreamFrame {
                id,
                stream_id,
                seq,
                kind,
                payload: Some(encode_stream_payload(&bytes[offset..end])),
                rows: None,
                columns: None,
            });
            let next = match from {
                // A retained frame's sequence is the absolute position of its
                // first byte, so the next frame starts where this one ended.
                Some(_) => seq.saturating_add((end - offset) as u64),
                // A source with no retained position numbers frames.
                None => seq.saturating_add(1),
            };
            self.next_seq.insert(stream_id, next);
            offset = end;
        }
        Ok(&bytes[offset..])
    }
    /// Continue `stream_id` at `next`, from the retained position the client
    /// asked to resume at. A position behind what this connection already
    /// published never rewinds the sequence.
    pub fn resume(&mut self, stream_id: u32, next: u64) {
        let current = self.next_seq.entry(stream_id).or_insert(0);
        if next > *current {
            *current = next;
        }
    }
    /// The receiver consumed up to `seq` on one stream: `false` when that
    /// sequence names no frame this connection published and left open.
    pub fn acknowledge(&mut self, stream_id: u32, seq: u64) -> bool {
        self.window.acknowledge(stream_id, seq)
    }
    pub fn take(&mut self) -> Vec<StreamFrame> {
        std::mem::take(&mut self.data)
    }
    pub fn window(&self) -> &StreamWindow {
        &self.window
    }
}

/// One captured chunk a connection may publish as data frames. The owner of
/// the run's logs supplies these; the channel only bounds and frames them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamChunk {
    pub stream_id: u32,
    pub kind: StreamKind,
    /// The absolute retained position of `bytes[0]` on `stream_id`, when the
    /// source reads a retained log. `None` is a source with no retained
    /// position, whose frames are numbered instead.
    pub cursor: Option<u64>,
    pub bytes: Vec<u8>,
}

/// Where every stream a connection carries starts publishing (052 §8.11).
/// Each stream has its own retained log, so each has its own position; a stream
/// whose log no longer holds the requested bytes starts where its log starts and
/// reports that bounded range as a gap before any data.
pub type StreamStart = BTreeMap<u32, u64>;

/// The positions a client reports it has already consumed: one default, and one
/// per stream id for the streams it read independently. Retained streams have
/// independent logs and lengths, so one position for a whole connection cannot
/// describe them — a client that reports only a default is resumed on the stream
/// that default belongs to and nothing else.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cursors {
    /// The position for the connection's primary stream, from `attach.cursor`.
    pub primary: u64,
    /// Consumed positions per stream id, which may differ from each other.
    pub streams: BTreeMap<u32, u64>,
}
impl Cursors {
    /// A fresh connection that has consumed nothing.
    pub fn fresh() -> Self {
        Self::default()
    }
    /// Whether this is a first read: no position named at all.
    pub fn is_fresh(&self) -> bool {
        self.primary == 0 && self.streams.is_empty()
    }
    /// The position named for `stream_id`, if the client named one.
    pub fn position(&self, stream_id: u32) -> Option<u64> {
        self.streams
            .get(&stream_id)
            .copied()
            .or((stream_id == TERMINAL_STREAM).then_some(self.primary))
    }
}

/// The attached run's terminal owner, as lane B implements it: the front end
/// that holds the terminal and hands validated bytes and sizes to it.
///
/// Delivery is fenced twice. The channel re-reads the attachment row
/// immediately before it calls here, and the live [`Attachment`] is handed over
/// rather than its identifier — so the owner's own last check before writing is
/// `Attachment::attached_observed`, and a row released, replaced or ended by its
/// run's closure in between reports `false` and the write is skipped. An owner
/// that ignores the handle is not a terminal this channel can promise anything
/// to.
///
/// The kernel has no per-run terminal writer — `run::Attachment` carries
/// retained logs and stop/escalate control, not a terminal input channel, and
/// the supervisor's `ChildStdin` is its own sentinel control pipe — so no kernel
/// type implements this. A host with no terminal owner uses [`NoTerminal`],
/// which refuses rather than silently dropping input.
pub trait Terminal {
    /// Deliver one validated terminal input frame, exactly once and in
    /// sequence order. The live attachment is handed over so the owner can
    /// fence delivery on its own lifetime (`Attachment::attached_observed`)
    /// instead of trusting an identifier.
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal>;
    /// Record the newest terminal size. The owner keeps the latest size; the
    /// connection holds no queue, so a burst cannot grow into a backlog.
    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal>;
}
/// A terminal owner for a host that has no terminal on this connection.
#[derive(Debug, Default)]
pub struct NoTerminal;
impl Terminal for NoTerminal {
    fn input(&mut self, _attachment: &Attachment, _bytes: &[u8]) -> Result<(), Refusal> {
        Err(Refusal::unavailable(
            "this host has no terminal to deliver input to",
        ))
    }
    fn resize(&mut self, _attachment: &Attachment, _size: TerminalSize) -> Result<(), Refusal> {
        Err(Refusal::unavailable("this host has no terminal to resize"))
    }
}

/// One connection's stream endpoint: the attached run's retained output on the
/// way out, and the terminal's input and resize on the way in. Lane B's front
/// end and retained reader implement it, so the channel frames and validates
/// bytes without owning either. [`RetainedStreams`] is the kernel's own
/// implementation over a run's captured logs, and [`NoStreams`] is a host with
/// nothing to publish.
pub trait Streams {
    /// The next chunk to publish, if any. The connection's live attachment is
    /// passed so the endpoint can re-check its own lifetime before reading.
    fn next_chunk(&mut self, attachment: Option<&Attachment>) -> Option<StreamChunk>;

    /// Where **every** stream this connection carries starts, for the positions
    /// the client reports it has consumed: `None` refuses the attach. It is
    /// asked before the attachment opens and is pure — nothing about the current
    /// reader moves — so a host that refuses the replacement leaves the
    /// connection's working attachment exactly where it was.
    ///
    /// The default is a source with nothing retained: it serves the start of the
    /// stream and refuses any later cursor, so a reconnect is never presented as
    /// an uninterrupted sequence from zero. A source that keeps several retained
    /// streams answers for each of them — refusing when the client's position
    /// for one cannot be established — so no stream silently replays consumed
    /// output or skips bytes it never read.
    fn resume(&mut self, _run: RunId, cursors: &Cursors) -> Option<StreamStart> {
        if cursors.is_fresh() {
            Some(StreamStart::from([(TERMINAL_STREAM, 0)]))
        } else {
            None
        }
    }

    /// Commit the negotiated start positions for the attachment that actually
    /// opened. A host that refused the attach never reaches here, so a refused
    /// replacement cannot move the reader the connection is still serving.
    fn start(&mut self, _attachment: &Attachment, _start: &StreamStart) {}

    /// Deliver one validated terminal input frame.
    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        let _ = (attachment, bytes);
        Err(Refusal::unavailable(
            "this host has no terminal to deliver input to",
        ))
    }

    /// Record the newest terminal size. A host without a terminal refuses
    /// rather than reporting a resize it did not apply.
    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        let _ = (attachment, size);
        Err(Refusal::unavailable("this host has no terminal to resize"))
    }
}
/// A host with nothing to publish on this connection and no terminal on it.
#[derive(Debug, Default)]
pub struct NoStreams;
impl Streams for NoStreams {
    fn next_chunk(&mut self, _attachment: Option<&Attachment>) -> Option<StreamChunk> {
        None
    }
}

/// The kernel's production stream endpoint for one attached run (§8.11): the
/// run's captured process logs read by absolute retained position, plus the
/// terminal owner the front end supplies.
///
/// The retained log is the only authority for a position. A resume cursor, a
/// frame sequence and a rotation gap are all byte offsets in the same log, so a
/// reconnect resumes exactly where the client stopped, a gap is reported from
/// the offsets the log no longer holds, and no replay store or second cursor
/// exists. The attachment's own lifetime is re-checked on every read, so a
/// released or replaced row stops publication.
#[derive(Debug)]
pub struct RetainedStreams<T: Terminal> {
    observe: ObserveContext,
    root: StateRoot,
    run: RunId,
    /// Stream id `n` names the `n`th captured stream here; the stream decides
    /// the kind of frames it publishes.
    plan: Vec<(i64, crate::custody::Stream)>,
    /// The next retained byte each stream id will publish.
    next: Vec<u64>,
    terminal: T,
}

impl<T: Terminal> RetainedStreams<T> {
    /// The endpoint for `run`'s captured streams, in stream-id order.
    pub fn new(
        observe: ObserveContext,
        root: StateRoot,
        run: RunId,
        plan: Vec<(i64, crate::custody::Stream)>,
        terminal: T,
    ) -> Self {
        let next = vec![0; plan.len()];
        Self {
            observe,
            root,
            run,
            plan,
            next,
            terminal,
        }
    }
    /// The run this endpoint serves.
    pub fn run(&self) -> RunId {
        self.run
    }
    /// The terminal owner this endpoint delivers input and resize to.
    pub fn terminal_mut(&mut self) -> &mut T {
        &mut self.terminal
    }
}

impl<T: Terminal> Streams for RetainedStreams<T> {
    fn next_chunk(&mut self, attachment: Option<&Attachment>) -> Option<StreamChunk> {
        let attachment = attachment?;
        // A released attachment, a replaced row or an unreadable journal stops
        // publication instead of reading past the client's lifetime.
        if !matches!(
            attachment.attached_observed(&self.observe, &self.root),
            Ok(true)
        ) {
            return None;
        }
        for index in 0..self.plan.len() {
            let (sequence, stream) = self.plan[index];
            let read = match run::read_log(
                &self.observe,
                &self.root,
                self.run,
                sequence,
                stream,
                self.next[index],
                MAX_STREAM_PAYLOAD,
            ) {
                Observed::Verified(read, _) => read,
                // No captured log for this stream, or one that cannot be read:
                // nothing to publish here rather than invented output.
                Observed::Absent(_) | Observed::Unknown(_) => continue,
            };
            let kind = match stream {
                crate::custody::Stream::Stdout => StreamKind::Stdout,
                crate::custody::Stream::Stderr => StreamKind::Stderr,
            };
            if read.bytes.is_empty() {
                if read.from > self.next[index] {
                    // The log no longer holds the bytes between the client's
                    // position and where it resumes, and this read returned
                    // none of them — an empty kept segment still means the range
                    // is gone. Publish the bounded gap on its own, so a run that
                    // ends here does not leave the client believing it saw
                    // everything.
                    self.next[index] = read.from;
                    return Some(StreamChunk {
                        stream_id: index as u32,
                        kind,
                        cursor: Some(read.from),
                        bytes: Vec::new(),
                    });
                }
                self.next[index] = read.end;
                continue;
            }
            self.next[index] = read.from.saturating_add(read.bytes.len() as u64);
            return Some(StreamChunk {
                stream_id: index as u32,
                kind,
                // The frame's position in the retained log, so the client's
                // cursor and the frame sequence are one coordinate system.
                cursor: Some(read.from),
                bytes: read.bytes,
            });
        }
        None
    }

    fn resume(&mut self, run: RunId, cursors: &Cursors) -> Option<StreamStart> {
        if run != self.run {
            return None;
        }
        // A position for a stream this endpoint does not publish cannot be
        // honoured, and ignoring it would open an attachment as if the client's
        // resume had been served.
        if cursors
            .streams
            .keys()
            .any(|stream_id| *stream_id as usize >= self.plan.len())
        {
            return None;
        }
        // Every stream this endpoint publishes is negotiated at its own
        // position: retained logs have independent lengths, so one position for
        // the whole connection would skip bytes the client never read on a
        // shorter stream, or replay bytes it had consumed on a longer one.
        let mut start = StreamStart::new();
        for (stream_id, &(sequence, stream)) in self.plan.iter().enumerate() {
            let stream_id = stream_id as u32;
            let requested = match cursors.position(stream_id) {
                Some(position) => position,
                // A stream the client never named cannot be resumed safely: it
                // may already have consumed part of it. Only a fresh read, which
                // reports no position at all, starts here.
                None if cursors.is_fresh() => 0,
                None => return None,
            };
            let retained = run::read_log(
                &self.observe,
                &self.root,
                run,
                sequence,
                stream,
                requested,
                1,
            );
            let position = match retained {
                // The log's end is its extent: a position at or before it is a
                // place this stream continues from.
                Observed::Verified(read, _) if requested <= read.end => requested,
                // A position past the retained end is a future one, refused
                // rather than clamped to the end (which would skip the bytes in
                // between) or restarted at zero.
                Observed::Verified(..) => return None,
                // No retained log: either nothing was captured yet or retention
                // removed it, and this read cannot tell which. A fresh read
                // starts at zero; a resume at any later position cannot be
                // established, so the attach is refused rather than opened as if
                // it had resumed.
                Observed::Absent(_) if requested == 0 => 0,
                Observed::Absent(_) | Observed::Unknown(_) => return None,
            };
            start.insert(stream_id, position);
        }
        Some(start)
    }

    fn start(&mut self, _attachment: &Attachment, start: &StreamStart) {
        // Committed only for the attachment that opened: a refused replacement
        // never reaches here, so the stream the connection is still serving
        // keeps its position.
        for (stream_id, position) in start {
            if let Some(next) = self.next.get_mut(*stream_id as usize) {
                *next = *position;
            }
        }
    }

    fn input(&mut self, attachment: &Attachment, bytes: &[u8]) -> Result<(), Refusal> {
        self.terminal.input(attachment, bytes)
    }

    fn resize(&mut self, attachment: &Attachment, size: TerminalSize) -> Result<(), Refusal> {
        self.terminal.resize(attachment, size)
    }
}

/// One frame on the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    Hello(Hello),
    HelloOk(HelloOk),
    Refuse(Refusal),
    Authenticate(Secret),
    Enrol(EnrolRequest),
    Request(Request),
    Response(Response),
    Stream(StreamFrame),
}
impl Frame {
    fn tag(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::HelloOk(_) => "hello_ok",
            Self::Refuse(_) => "refuse",
            Self::Authenticate(_) => "authenticate",
            Self::Enrol(_) => "enrol",
            Self::Request(_) => "request",
            Self::Response(_) => "response",
            Self::Stream(_) => "stream",
        }
    }
}

/// Encode a frame body. Framing (the four-byte length prefix) belongs to the
/// connection, so a body and its length can never disagree.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, Refusal> {
    let mut object = Map::new();
    object.insert("frame".to_string(), Value::String(frame.tag().into()));
    match frame {
        Frame::Hello(value) => insert(&mut object, value)?,
        Frame::HelloOk(value) => insert(&mut object, value)?,
        Frame::Refuse(value) => insert(&mut object, value)?,
        Frame::Authenticate(secret) => {
            object.insert(
                "capability".to_string(),
                serde_json::to_value(secret).map_err(not_encodable)?,
            );
        }
        Frame::Enrol(value) => insert(&mut object, value)?,
        Frame::Request(value) => {
            object.insert(
                "id".to_string(),
                serde_json::to_value(value.id).map_err(not_encodable)?,
            );
            object.insert(
                "method".to_string(),
                serde_json::to_value(value.method()).map_err(not_encodable)?,
            );
            object.insert(
                "context_generation".to_string(),
                serde_json::to_value(value.context_generation).map_err(not_encodable)?,
            );
            object.insert(
                "environment_generation".to_string(),
                serde_json::to_value(value.environment_generation).map_err(not_encodable)?,
            );
            object.insert(
                "session_run_id".to_string(),
                serde_json::to_value(value.session_run_id).map_err(not_encodable)?,
            );
            object.insert(
                "container_id".to_string(),
                Value::String(value.container_id.clone()),
            );
            object.insert(
                "vm_boot_id".to_string(),
                Value::String(value.vm_boot_id.clone()),
            );
            object.insert(
                "body".to_string(),
                serde_json::to_value(&value.body).map_err(not_encodable)?,
            );
        }
        Frame::Response(value) => {
            object.insert(
                "id".to_string(),
                serde_json::to_value(value.id).map_err(not_encodable)?,
            );
            match &value.outcome {
                ResponseOutcome::Observed(observed) => {
                    object.insert("observed".to_string(), observed.clone())
                }
                ResponseOutcome::Accepted(accepted) => object.insert(
                    "accepted".to_string(),
                    serde_json::to_value(accepted).map_err(not_encodable)?,
                ),
                ResponseOutcome::Completed(completed) => object.insert(
                    "completed".to_string(),
                    serde_json::to_value(completed).map_err(not_encodable)?,
                ),
                ResponseOutcome::Refused(refusal) => object.insert(
                    "refused".to_string(),
                    serde_json::to_value(refusal).map_err(not_encodable)?,
                ),
            };
        }
        Frame::Stream(value) => insert(&mut object, value)?,
    }
    let body = serde_json::to_vec(&Value::Object(object)).map_err(not_encodable)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(Refusal::invalid("a frame is at most 1 MiB"));
    }
    Ok(body)
}
fn not_encodable(_: serde_json::Error) -> Refusal {
    Refusal::invalid("a frame is not encodable")
}
fn insert<T: Serialize>(object: &mut Map<String, Value>, value: &T) -> Result<(), Refusal> {
    let Value::Object(fields) = serde_json::to_value(value).map_err(not_encodable)? else {
        return Err(Refusal::invalid("a frame payload is an object"));
    };
    for (key, field) in fields {
        if key == "frame" {
            return Err(Refusal::invalid("a frame payload shadows its own tag"));
        }
        object.insert(key, field);
    }
    Ok(())
}

/// Decode one frame body. Malformed JSON, a repeated key, an unknown frame or
/// method, an unknown field or an excess limit refuses here, before dispatch.
pub fn decode_frame(body: &[u8]) -> Result<Frame, Refusal> {
    if body.len() > MAX_FRAME_BYTES {
        return Err(Refusal::invalid("a frame is at most 1 MiB"));
    }
    let value = serde_json::from_slice::<NoDuplicates>(body)
        .map_err(|_| Refusal::invalid("a frame is not JSON, or repeats a key"))?
        .0;
    let Value::Object(mut object) = value else {
        return Err(Refusal::invalid("a frame is a JSON object"));
    };
    let tag = match object.remove("frame") {
        Some(Value::String(tag)) => tag,
        _ => return Err(Refusal::invalid("a frame names its kind in `frame`")),
    };
    let frame = match tag.as_str() {
        "hello" => Frame::Hello(parse(object)?),
        "hello_ok" => Frame::HelloOk(parse(object)?),
        "refuse" => Frame::Refuse(parse(object)?),
        "authenticate" => {
            let Some(capability) = object.remove("capability") else {
                return Err(Refusal::invalid(
                    "an authenticate frame carries a capability",
                ));
            };
            if !object.is_empty() {
                return Err(Refusal::invalid(
                    "an authenticate frame carries only its capability",
                ));
            }
            Frame::Authenticate(
                serde_json::from_value(capability)
                    .map_err(|_| Refusal::invalid("a capability is a secret"))?,
            )
        }
        "enrol" => Frame::Enrol(parse(object)?),
        "request" => Frame::Request(parse_request(object)?),
        "response" => Frame::Response(parse_response(object)?),
        "stream" => Frame::Stream(parse(object)?),
        _ => {
            return Err(Refusal::invalid(
                "a frame kind is one this protocol defines",
            ))
        }
    };
    match &frame {
        Frame::Request(request) => request.body.check()?,
        Frame::Stream(stream) => stream.check()?,
        Frame::Hello(hello) => {
            if hello.protocol_versions.len() > 8 {
                return Err(Refusal::invalid("a hello names at most eight versions"));
            }
        }
        Frame::Enrol(enrol)
            if enrol.container_id.len() > MAX_PATH_BYTES
                || enrol.vm_boot_id.len() > MAX_PATH_BYTES =>
        {
            return Err(Refusal::invalid("an enrol frame is bounded"));
        }
        _ => {}
    }
    Ok(frame)
}
fn parse<T: for<'de> Deserialize<'de>>(object: Map<String, Value>) -> Result<T, Refusal> {
    serde_json::from_value(Value::Object(object))
        .map_err(|_| Refusal::invalid("a frame carries its documented fields and no others"))
}
fn parse_request(mut object: Map<String, Value>) -> Result<Request, Refusal> {
    let method: Method = match object.remove("method") {
        Some(value) => serde_json::from_value(value)
            .map_err(|_| Refusal::invalid("a request names a method this protocol defines"))?,
        None => return Err(Refusal::invalid("a request names its method")),
    };
    let Some(body) = object.remove("body") else {
        return Err(Refusal::invalid("a request carries a body"));
    };
    let body = match method {
        Method::Observe => RequestBody::Observe(serde_json::from_value(body).map_err(|_| {
            Refusal::invalid("an observe body carries its documented fields and no others")
        })?),
        Method::Invoke => RequestBody::Invoke(serde_json::from_value(body).map_err(|_| {
            Refusal::invalid("an invocation body carries its documented fields and no others")
        })?),
        Method::Attach => RequestBody::Attach(serde_json::from_value(body).map_err(|_| {
            Refusal::invalid("an attach body carries its documented fields and no others")
        })?),
        Method::Cancel => RequestBody::Cancel(serde_json::from_value(body).map_err(|_| {
            Refusal::invalid("a cancel body carries its documented fields and no others")
        })?),
        Method::Detach => RequestBody::Detach(serde_json::from_value(body).map_err(|_| {
            Refusal::invalid("a detach body carries its documented fields and no others")
        })?),
    };
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        id: RequestId,
        context_generation: Generation,
        environment_generation: Generation,
        session_run_id: RunId,
        container_id: String,
        vm_boot_id: String,
    }
    let fields: Fields = serde_json::from_value(Value::Object(object))
        .map_err(|_| Refusal::invalid("a request carries its documented fields and no others"))?;
    if fields.container_id.len() > MAX_PATH_BYTES || fields.vm_boot_id.len() > MAX_PATH_BYTES {
        return Err(Refusal::invalid("a request's identity fields are bounded"));
    }
    Ok(Request {
        id: fields.id,
        context_generation: fields.context_generation,
        environment_generation: fields.environment_generation,
        session_run_id: fields.session_run_id,
        container_id: fields.container_id,
        vm_boot_id: fields.vm_boot_id,
        body,
    })
}
fn parse_response(mut object: Map<String, Value>) -> Result<Response, Refusal> {
    let Some(id) = object.remove("id") else {
        return Err(Refusal::invalid("a response carries its request id"));
    };
    let id = serde_json::from_value(id)
        .map_err(|_| Refusal::invalid("a response carries its request id"))?;
    let mut outcome = None;
    for (key, value) in std::mem::take(&mut object) {
        let parsed = match key.as_str() {
            "observed" => ResponseOutcome::Observed(value),
            "accepted" => {
                ResponseOutcome::Accepted(serde_json::from_value(value).map_err(|_| {
                    Refusal::invalid("an accepted response carries a run id and placement")
                })?)
            }
            "completed" => {
                ResponseOutcome::Completed(serde_json::from_value(value).map_err(|_| {
                    Refusal::invalid("a completed response carries an outcome and exit status")
                })?)
            }
            "refused" => ResponseOutcome::Refused(
                serde_json::from_value(value)
                    .map_err(|_| Refusal::invalid("a refused response carries a refusal"))?,
            ),
            _ => return Err(Refusal::invalid("a response carries exactly one outcome")),
        };
        if outcome.replace(parsed).is_some() {
            return Err(Refusal::invalid("a response carries exactly one outcome"));
        }
    }
    let outcome =
        outcome.ok_or_else(|| Refusal::invalid("a response carries exactly one outcome"))?;
    Ok(Response { id, outcome })
}

/// Deserialises any JSON value while refusing a repeated key in one object.
/// `serde_json` keeps the last duplicate silently, so a frame that repeats a
/// key must refuse before the typed shape is built.
struct NoDuplicates(Value);
impl<'de> Deserialize<'de> for NoDuplicates {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NoDuplicatesVisitor)
    }
}
struct NoDuplicatesVisitor;
impl<'de> Visitor<'de> for NoDuplicatesVisitor {
    type Value = NoDuplicates;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value with no repeated object key")
    }
    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::Bool(value)))
    }
    fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::Number(value.into())))
    }
    fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::Number(value.into())))
    }
    fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(|number| NoDuplicates(Value::Number(number)))
            .ok_or_else(|| E::custom("a frame carries no non-finite number"))
    }
    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::String(value.to_owned())))
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::Null))
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok(NoDuplicates(Value::Null))
    }
    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        NoDuplicates::deserialize(deserializer)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<NoDuplicates>()? {
            values.push(value.0);
        }
        Ok(NoDuplicates(Value::Array(values)))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut fields = Map::new();
        let mut seen = BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom("a frame repeats a key"));
            }
            let value: NoDuplicates = map.next_value()?;
            fields.insert(key, value.0);
        }
        Ok(NoDuplicates(Value::Object(fields)))
    }
}

/// A response carries its request id and exactly one outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub id: RequestId,
    pub outcome: ResponseOutcome,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponseOutcome {
    Observed(Value),
    Accepted(Accepted),
    Completed(Completed),
    Refused(Refusal),
}
impl Response {
    pub fn refused(id: RequestId, refusal: Refusal) -> Self {
        Self {
            id,
            outcome: ResponseOutcome::Refused(refusal),
        }
    }
}
/// The host admitted a run for this request. `placement` is §8.1's host/guest
/// placement, which the journal records; a channel admission is always a host
/// run and a guest placement is refused before it is reached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accepted {
    pub run: RunId,
    pub placement: Placement,
}
/// The run ended. The outcome is the host's recorded outcome, unchanged; the
/// channel never re-renders it into a frame of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completed {
    pub outcome: Value,
    pub exit: ExitStatus,
}
/// How the workload ended, where the host recorded it. `None`/`None` is "not
/// recorded", never success.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExitStatus {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}
impl ExitStatus {
    pub const UNKNOWN: Self = Self {
        code: None,
        signal: None,
    };
}

/// The connection below the channel: an authenticated byte stream. The
/// integration owns TLS and reports the identity TLS verified; this module
/// owns framing, every limit and every decision about what may cross.
pub trait Transport {
    /// Read up to `buf.len()` bytes. `Ok(0)` is end of stream.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()>;
    /// The identity TLS verified for this connection, if any.
    fn verified_peer(&self) -> Option<ServerIdentity>;
}

/// Distinguish a local transport failure from a peer-supplied refusal code.
struct TrackedTransport<'a, T> {
    inner: &'a mut T,
    failed: bool,
}
impl<T: Transport> Transport for TrackedTransport<'_, T> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        let result = self.inner.read(bytes);
        self.failed |= result.is_err();
        result
    }
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        let result = self.inner.write_all(bytes);
        self.failed |= result.is_err();
        result
    }
    fn verified_peer(&self) -> Option<ServerIdentity> {
        self.inner.verified_peer()
    }
}

/// Read one framed body: four-byte big-endian length, checked against
/// [`MAX_FRAME_BYTES`] before the body is allocated.
pub fn read_frame<T: Transport>(transport: &mut T) -> Result<Option<Vec<u8>>, Refusal> {
    let mut prefix = [0u8; 4];
    let mut filled = 0;
    while filled < prefix.len() {
        match transport
            .read(&mut prefix[filled..])
            .map_err(|_| Refusal::unavailable("the connection failed while reading"))
        {
            Ok(0) if filled == 0 => return Ok(None),
            Ok(0) => return Err(Refusal::invalid("a frame ended inside its length prefix")),
            Ok(read) => filled += read,
            Err(refusal) => return Err(refusal),
        }
    }
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(Refusal::invalid("a frame is between 1 byte and 1 MiB"));
    }
    let mut body = vec![0u8; length];
    let mut filled = 0;
    while filled < length {
        match transport
            .read(&mut body[filled..])
            .map_err(|_| Refusal::unavailable("the connection failed while reading"))
        {
            Ok(0) => return Err(Refusal::invalid("a frame ended inside its body")),
            Ok(read) => filled += read,
            Err(refusal) => return Err(refusal),
        }
    }
    Ok(Some(body))
}
/// Write one framed frame.
pub fn write_frame<T: Transport>(transport: &mut T, frame: &Frame) -> Result<(), Refusal> {
    let body = encode_frame(frame)?;
    let mut framed = Vec::with_capacity(4 + body.len());
    framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
    framed.extend_from_slice(&body);
    transport
        .write_all(&framed)
        .map_err(|_| Refusal::unavailable("the connection failed while writing"))
}

/// One request id source. Entropy is engine-owned (§5.6): the channel asks for
/// one fresh run id per admission and records it in the reservation.
pub trait RunIds {
    fn next(&mut self) -> Result<RunId, Refusal>;
}

/// Re-resolution of a guest request into a host invocation. The owning engine
/// answers from host records; the channel never resolves a selector itself.
pub trait Resolver {
    fn resolve(
        &mut self,
        cx: &EffectContext,
        body: &InvocationBody,
    ) -> Result<ResolvedInvocation, Refusal>;

    /// Derive the exact invocation the engine will admit for this run.
    fn admission_invocation(
        &mut self,
        _body: &InvocationBody,
        resolved: &ResolvedInvocation,
        _run: RunId,
    ) -> Result<ResolvedInvocation, Refusal> {
        Ok(resolved.clone())
    }
}

/// The host engine's admission handoff: the dispatcher has reserved the
/// request's run and hands that ticket to the engine, which admits exactly the
/// run the reservation names (`Run::begin_admitted`).
pub trait Admission {
    fn admit(
        &mut self,
        cx: &EffectContext,
        ticket: &AdmissionTicket,
        resolved: &ResolvedInvocation,
    ) -> Result<(), Refusal>;
}

/// The host's read surface. Observe runs under [`ObserveContext`] and writes
/// nothing.
pub trait Observer {
    fn observe(&mut self, cx: &ObserveContext, body: &ObserveBody) -> Result<Value, Refusal>;
}

/// What an invocation admitted. A reservation whose run is already admitted is
/// reported as accepted; a recorded outcome is completed; anything the host
/// cannot establish is `unknown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionReply {
    Accepted(Accepted),
    Completed(Completed),
}

/// The typed host entry points. `invoke` reserves the request's key before it
/// touches anything else, so a replay reads the host's record instead of
/// admitting a second run.
#[derive(Debug)]
pub struct Dispatcher<A, R, O, I> {
    admission: A,
    resolver: R,
    observer: O,
    ids: I,
    /// Runs this connection admitted or recovered as admitted. Only they make
    /// this connection the owning client; every other attach is secondary.
    admitted: BTreeSet<RunId>,
}
impl<A: Admission, R: Resolver, O: Observer, I: RunIds> Dispatcher<A, R, O, I> {
    pub fn new(admission: A, resolver: R, observer: O, ids: I) -> Self {
        Self {
            admission,
            resolver,
            observer,
            ids,
            admitted: BTreeSet::new(),
        }
    }
    pub fn observer_mut(&mut self) -> &mut O {
        &mut self.observer
    }

    /// `observe` is host `ObserveContext` only: no run, reservation, token,
    /// journal entry or access log (§8.11).
    pub fn observe(&mut self, cx: &ObserveContext, body: &ObserveBody) -> Result<Value, Refusal> {
        body.check()?;
        self.observer.observe(cx, body)
    }

    /// Admit the host-resolved invocation for this keyed request. The
    /// reservation is written first; the engine then admits the run the
    /// reservation names. A replay with the same digest returns what the host
    /// recorded; a different digest is `id_conflict`; an unresolvable record is
    /// `unknown` and is never readmitted.
    #[allow(clippy::too_many_arguments)]
    pub fn invoke(
        &mut self,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        session: Session,
        id: RequestId,
        digest: Digest,
        body: &InvocationBody,
    ) -> Result<AdmissionReply, Refusal> {
        body.check()?;
        if cx.operation().placement() != Placement::Host {
            return Err(Refusal::forbidden(
                "a guest placement cannot dispatch a host admission",
            ));
        }
        let key = run::AdmissionKey {
            session,
            request: id,
        };
        // The record answers first: a retry never spends entropy or asks an
        // engine to admit, and an unreadable record is `unknown`, not a fresh
        // attempt (§8.11).
        match run::lookup(observe, root, key, digest.clone()) {
            Observed::Verified(recovered, _) => return replay(recovered),
            Observed::Absent(_) => {}
            Observed::Unknown(_) => {
                return Err(Refusal::unknown(
                    "the host could not establish whether this request was admitted; it will not admit a second run",
                ))
            }
        }
        // Resolution is a read of host records, and happens after the record
        // check so a recorded key answers with what it recorded. It also
        // happens before the reservation, so a selector the host refuses
        // records nothing and a corrected retry is a fresh request.
        let resolved = self.resolver.resolve(cx, body)?;
        let run = self.ids.next()?;
        let admission_invocation = self.resolver.admission_invocation(body, &resolved, run)?;
        let reply = match run::reserve_resolved(cx, root, key, digest, run, &admission_invocation)
            .map_err(refusal_from)?
        {
            Reservation::Recorded(recovered) => replay(recovered)?,
            Reservation::Fresh(ticket) => {
                // A refused admission keeps the reservation recorded against
                // this key, so a later retry reads `unknown` instead of
                // admitting a second run under a fresh id.
                self.admission.admit(cx, &ticket, &resolved)?;
                self.admitted.insert(ticket.run());
                AdmissionReply::Accepted(Accepted {
                    run: ticket.run(),
                    placement: Placement::Host,
                })
            }
        };
        Ok(reply)
    }

    /// Open this client's attachment to a run this session admitted. The
    /// attachment id is derived from the request id, so a lost response is
    /// resumed rather than allocated twice.
    pub fn attach(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        session: Session,
        id: RequestId,
        body: &AttachBody,
    ) -> Result<Attachment, Refusal> {
        body.check()?;
        // The connection that admitted the run is its owning client; every
        // other terminal takes a secondary attachment, which never stops,
        // detaches or releases the owner's run (§8.11).
        if self.admitted.contains(&body.run) {
            return run::resume_owner(cx, root, session, body.run).map_err(refusal_from);
        }
        let attachment = AttachmentId::new(*id.bytes());
        match run::resume_secondary(cx, root, session, body.run, attachment) {
            Ok(handle) => Ok(handle),
            Err(_) => {
                run::attach_secondary(cx, root, session, body.run, attachment).map_err(refusal_from)
            }
        }
    }

    /// Reopen the owning client's handle on a run this session admitted, for a
    /// reconnect after a restart. The kernel refuses any other run, and a
    /// secondary terminal never becomes the owner this way.
    pub fn reopen_owner(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        session: Session,
        run: RunId,
    ) -> Result<Attachment, Refusal> {
        if !self.admitted.contains(&run) {
            return Err(Refusal::forbidden("this connection did not admit the run"));
        }
        let handle = run::resume_owner(cx, root, session, run).map_err(refusal_from)?;
        Ok(handle)
    }

    /// Request stop of a run this client may steer. Without a handle for this
    /// run, the owning client's handle is reopened, which the kernel refuses
    /// for a run this session did not admit.
    pub fn cancel(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        session: Session,
        handle: Option<&Attachment>,
        body: &RunBody,
    ) -> Result<(), Refusal> {
        match handle {
            Some(handle) if handle.run() == body.run => {
                let id = self.ids.next()?;
                handle.control(cx, root, id, RequestAction::Stop)
            }
            _ => {
                if !self.admitted.contains(&body.run) {
                    return Err(Refusal::forbidden(
                        "this connection has no attachment to the run",
                    ));
                }
                let id = self.ids.next()?;
                let owner = run::resume_owner(cx, root, session, body.run).map_err(refusal_from)?;
                // The stop request is the whole effect: the handle is not
                // released here, so this client cannot end another client's
                // attachment on its way past.
                owner.control(cx, root, id, RequestAction::Stop)
            }
        }
        .map_err(refusal_from)
    }

    /// Acknowledge detach of this client's owned run without stopping it. The
    /// kernel records the detach apart from an EOF, so the run keeps going.
    pub fn detach(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        session: Session,
        body: &RunBody,
    ) -> Result<(), Refusal> {
        if !self.admitted.contains(&body.run) {
            return Err(Refusal::forbidden("this connection did not admit the run"));
        }
        let mut owner = run::resume_owner(cx, root, session, body.run).map_err(refusal_from)?;
        owner
            .release(cx, root, Release::Detach)
            .map_err(refusal_from)
    }
}

/// Map a recorded reservation to the answer the client gets. `Unknown` is an
/// answer, and it never admits.
fn replay(recovered: Recovered) -> Result<AdmissionReply, Refusal> {
    match recovered {
        Recovered::Admitted { run, .. } => Ok(AdmissionReply::Accepted(Accepted {
            run,
            placement: Placement::Host,
        })),
        Recovered::Completed { run: _, outcome } => Ok(AdmissionReply::Completed(completed(outcome)?)),
        Recovered::Unknown { reason } => Err(match reason {
            UnknownAdmission::ReservedNotAdmitted => Refusal::unknown(
                "this request was reserved and no admission can be established for it; the host will not admit a second run",
            ),
            UnknownAdmission::AdmissionNotRecovered => Refusal::unknown(
                "the run for this request cannot be established and no outcome is recorded; the host will not admit a second run",
            ),
        }),
        Recovered::Conflict { .. } => Err(Refusal::new(
            RefusalCode::IdConflict,
            "this request id was used for a different request in this session",
        )),
    }
}
fn completed(outcome: RecordedOutcome) -> Result<Completed, Refusal> {
    Ok(Completed {
        outcome: serde_json::to_value(&outcome)
            .map_err(|_| Refusal::unknown("a recorded outcome is not readable"))?,
        exit: ExitStatus::UNKNOWN,
    })
}

/// A kernel error becomes a stable refusal code with a sentence that names no
/// host path and no credential.
fn refusal_from(error: RunError) -> Refusal {
    match error {
        RunError::HeldBy(holders) => Refusal::new(
            RefusalCode::HeldBy,
            format!("the run is held by {} other holder(s)", holders.len()),
        ),
        RunError::StaleGeneration { .. } => {
            Refusal::stale("the run or session moved to another generation; reread the context")
        }
        RunError::Interrupted(_) => {
            Refusal::unknown("the host stopped before this request was answered")
        }
        RunError::Unavailable(_) => {
            Refusal::unavailable("the host could not answer this request; retry once it is quieter")
        }
        RunError::Broken(_) | RunError::Store(_) => {
            Refusal::unknown("the host could not establish its own record for this request")
        }
        RunError::Refused(rule) => Refusal::forbidden(rule),
        RunError::Gate(_) => Refusal::forbidden("the run's gate refused this request"),
        RunError::EnvNotActive => Refusal::unavailable("the environment is not active"),
        RunError::PredecessorUnsettled { .. } => {
            Refusal::new(RefusalCode::HeldBy, "an earlier run is still settling")
        }
        RunError::Reported { category, .. } => match category {
            crate::human::Category::Refused => Refusal::forbidden("the host refused this request"),
            crate::human::Category::Unavailable => {
                Refusal::unavailable("the host could not answer this request")
            }
            crate::human::Category::Broken => {
                Refusal::unknown("the host could not establish its own record")
            }
        },
    }
}

/// One connection's server side: the negotiated facts, the session fence, the
/// grants and the dispatcher. `serve` is the whole host conversation.
pub struct Server<A: Admission, R: Resolver, O: Observer, I: RunIds> {
    negotiation: Negotiation,
    fence: SessionFence,
    pinned: Digest,
    grants: GrantBook,
    dispatcher: Dispatcher<A, R, O, I>,
    /// Do not serve another client while an owner's EOF cleanup is uncertain.
    owner_cleanup_unsettled: bool,
    unsettled_attachment: Option<Attachment>,
    unsettled_replacement: Option<Attachment>,
    pending_eof_root: Option<crate::fs::DirectoryIdentity>,
    pending_eof_owners: BTreeMap<RunId, Attachment>,
    /// Data frames this connection has queued and the client has not yet
    /// acknowledged, plus the request id they name.
    outbound: Outbound,
    /// The attach request this connection's attachment was opened for. A
    /// terminal frame names it, so a frame for another request never reaches
    /// the terminal.
    attach: Option<RequestId>,
    carry: Option<StreamChunk>,
    /// The next inbound sequence this connection accepts per stream id:
    /// terminal input and resize, in order and never replayed.
    inbound: BTreeMap<u32, u64>,
}
impl<A: Admission, R: Resolver, O: Observer, I: RunIds> fmt::Debug for Server<A, R, O, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Server")
            .field("negotiation", &self.negotiation)
            .field("fence", &self.fence)
            .field("pinned", &self.pinned)
            .finish_non_exhaustive()
    }
}
/// What one connection did. `Closed` is an ordinary end of stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    pub responses: usize,
    pub streams: usize,
    pub closed: bool,
}
impl<A: Admission, R: Resolver, O: Observer, I: RunIds> Server<A, R, O, I> {
    /// `pinned` is the server identity the selected context pinned. TLS must
    /// have verified it; [`Transport::verified_peer`] reports what it verified.
    pub fn new(
        negotiation: Negotiation,
        fence: SessionFence,
        pinned: Digest,
        grants: GrantBook,
        dispatcher: Dispatcher<A, R, O, I>,
    ) -> Self {
        Self {
            negotiation,
            fence,
            pinned,
            grants,
            dispatcher,
            owner_cleanup_unsettled: false,
            unsettled_attachment: None,
            unsettled_replacement: None,
            pending_eof_root: None,
            pending_eof_owners: BTreeMap::new(),
            outbound: Outbound::new(),
            attach: None,
            carry: None,
            inbound: BTreeMap::new(),
        }
    }
    pub fn grants(&self) -> &GrantBook {
        &self.grants
    }
    pub fn grants_mut(&mut self) -> &mut GrantBook {
        &mut self.grants
    }

    /// Whether this server retains an attachment or admission whose disconnect
    /// cleanup could not be established. The serving owner must retain this
    /// server and its run custody while the state remains unresolved.
    pub fn cleanup_pending(&self) -> bool {
        self.owner_cleanup_unsettled
    }

    /// Retry retained attachment releases with fresh cleanup authority. A
    /// verified EOF can also drain the exact owner handles captured at that
    /// disconnect. Unknown and malformed-owner states remain fenced; this
    /// does not settle the session or any child run.
    pub fn retry_cleanup(&mut self, cx: &EffectContext, root: &StateRoot) -> Result<(), Refusal> {
        if !self.owner_cleanup_unsettled {
            return Err(Refusal::forbidden("there is no retained channel cleanup"));
        }
        if let Some(replacement) = self.unsettled_replacement.as_mut() {
            if let Some(previous) = self.unsettled_attachment.as_mut() {
                if previous.client() != Client::Secondary
                    || previous.pending_release() != Some(Release::Eof)
                {
                    return Err(Refusal::unknown(
                        "attachment replacement cleanup is unsettled",
                    ));
                }
                previous
                    .release_recoverable(cx, root, Release::Eof)
                    .map_err(|_| Refusal::unknown("attachment replacement cleanup is unsettled"))?;
                self.unsettled_attachment = None;
            }
            if replacement.client() != Client::Secondary {
                return Err(Refusal::unknown(
                    "attachment replacement cleanup is unsettled",
                ));
            }
            let released = match replacement.pending_release() {
                None => replacement.release(cx, root, Release::Eof),
                Some(Release::Eof) => replacement.release_recoverable(cx, root, Release::Eof),
                Some(Release::Detach) => {
                    return Err(Refusal::unknown(
                        "attachment replacement cleanup is unsettled",
                    ))
                }
            };
            released
                .map_err(|_| Refusal::unknown("attachment replacement cleanup is unsettled"))?;
            self.unsettled_replacement = None;
            return self.retry_pending_owners(cx, root);
        }
        let Some(handle) = self.unsettled_attachment.as_mut() else {
            return self.retry_pending_owners(cx, root);
        };
        let Some(cause) = handle.pending_release() else {
            return Err(Refusal::unknown(
                "attachment release cause is not established",
            ));
        };
        handle
            .release_recoverable(cx, root, cause)
            .map_err(|_| Refusal::unknown("attachment cleanup is unsettled"))?;
        if handle.client() == Client::Owner {
            self.dispatcher.admitted.remove(&handle.run());
        }
        self.unsettled_attachment = None;
        self.retry_pending_owners(cx, root)
    }

    fn retry_pending_owners(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
    ) -> Result<(), Refusal> {
        if !self.dispatcher.admitted.is_empty() {
            let identity = root
                .state_directory()
                .identity()
                .map_err(|_| Refusal::unknown("owner cleanup root is unavailable"))?;
            if self.pending_eof_root != Some(identity) {
                return Err(Refusal::unknown(
                    "owner EOF authority is not established for this root",
                ));
            }
            for run in self.dispatcher.admitted.clone() {
                let Some(mut owner) = self.pending_eof_owners.remove(&run) else {
                    return Err(Refusal::unknown("original owner attachment is unavailable"));
                };
                if owner.release(cx, root, Release::Eof).is_err() {
                    if owner.pending_release().is_some() {
                        self.unsettled_attachment = Some(owner);
                    } else {
                        self.pending_eof_owners.insert(run, owner);
                    }
                    return Err(Refusal::unknown("owner disconnect cleanup is unsettled"));
                }
                self.dispatcher.admitted.remove(&run);
            }
        }
        self.pending_eof_root = None;
        self.pending_eof_owners.clear();
        self.owner_cleanup_unsettled = false;
        Ok(())
    }

    /// Serve one connection: pin check, hello, negotiation, authentication,
    /// then requests. Nothing is read or written before the pin check, and no
    /// request is answered before authentication.
    pub fn serve<T: Transport>(
        &mut self,
        transport: &mut T,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        streams: &mut impl Streams,
    ) -> Result<Served, Refusal> {
        if self.owner_cleanup_unsettled {
            return Err(Refusal::unknown("owner disconnect cleanup is unsettled"));
        }
        // A server may serve successive connections, so each one starts with no
        // queued stream state: a previous connection's window, carry or request
        // id never reaches this one.
        self.begin_connection();
        // The pin is decided before a single byte crosses the connection, so a
        // mismatched identity never receives a credential and never learns a
        // version.
        if let Err(refusal) = check_server_pin(&self.pinned, transport.verified_peer().as_ref()) {
            let _ = write_frame(transport, &Frame::Refuse(refusal.clone()));
            return Err(refusal);
        }
        match self.handshake(transport)? {
            Some(grant) => {
                let mut state = Served {
                    responses: 0,
                    streams: 0,
                    closed: false,
                };
                self.converse(transport, cx, observe, root, grant, streams, &mut state)?;
                Ok(state)
            }
            None => Ok(Served {
                responses: 0,
                streams: 0,
                closed: true,
            }),
        }
    }

    /// Drop everything the previous connection left queued.
    fn begin_connection(&mut self) {
        // Admission ownership belongs only to the connection that admitted it.
        // A replay on another connection cannot recover an owner attachment.
        self.dispatcher.admitted.clear();
        self.pending_eof_root = None;
        self.pending_eof_owners.clear();
        self.outbound = Outbound::new();
        self.attach = None;
        self.carry = None;
        self.inbound.clear();
    }

    /// The unauthenticated half: `hello`, `hello_ok`, then `authenticate` or
    /// `enrol`. A refusal here is written once and ends the connection.
    fn handshake<T: Transport>(&mut self, transport: &mut T) -> Result<Option<Grant>, Refusal> {
        let Some(body) = read_frame(transport)? else {
            return Ok(None);
        };
        let hello = match decode_frame(&body) {
            Ok(Frame::Hello(hello)) => hello,
            Ok(_) => {
                return Err(self.refuse(
                    transport,
                    Refusal::invalid("a connection begins with `hello`"),
                ))
            }
            Err(refusal) => return Err(self.refuse(transport, refusal)),
        };
        match negotiate(&hello, &self.negotiation) {
            Ok(ok) => write_frame(transport, &Frame::HelloOk(ok))?,
            Err(refusal) => return Err(self.refuse(transport, refusal)),
        }
        let Some(body) = read_frame(transport)? else {
            return Ok(None);
        };
        let frame = match decode_frame(&body) {
            Ok(frame) => frame,
            Err(refusal) => return Err(self.refuse(transport, refusal)),
        };
        match frame {
            Frame::Authenticate(secret) => match self.grants.authenticate(&secret) {
                Ok(grant) => Ok(Some(grant.clone())),
                Err(refusal) => Err(self.refuse(transport, refusal)),
            },
            // Enrolment issues a capability and executes nothing. The client
            // learns it worked from its next request; no credential is echoed.
            Frame::Enrol(enrol) => match self.grants.enrol(&enrol.credential, &enrol) {
                Ok(grant) => Ok(Some(grant)),
                Err(refusal) => Err(self.refuse(transport, refusal)),
            },
            _ => Err(self.refuse(
                transport,
                Refusal::unauthenticated("a connection authenticates or enrols before it requests"),
            )),
        }
    }

    /// The authenticated half. Each request is fenced, authorised, dispatched
    /// and answered; a client refusal or end of stream releases the client's
    /// attachment.
    #[allow(clippy::too_many_arguments)]
    fn converse<T: Transport>(
        &mut self,
        transport: &mut T,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        grant: Grant,
        streams: &mut impl Streams,
        state: &mut Served,
    ) -> Result<(), Refusal> {
        let mut attachment: Option<Attachment> = None;
        let mut tracked = TrackedTransport {
            inner: transport,
            failed: false,
        };
        let outcome = self.converse_frames(
            &mut tracked,
            cx,
            observe,
            root,
            grant,
            streams,
            state,
            &mut attachment,
        );
        let transport_lost = tracked.failed
            && outcome
                .as_ref()
                .is_err_and(|error| error.code == RefusalCode::Unavailable);
        if state.closed || transport_lost {
            match root.state_directory().identity() {
                Ok(identity) => self.pending_eof_root = Some(identity),
                Err(_) => {
                    self.unsettled_attachment = attachment.take();
                    self.owner_cleanup_unsettled = true;
                    return Err(Refusal::unknown("owner cleanup root is unavailable"));
                }
            }
            if attachment.as_ref().is_some_and(|handle| {
                handle.client() == Client::Owner && self.dispatcher.admitted.contains(&handle.run())
            }) {
                let handle = attachment.take().expect("owner attachment was present");
                self.pending_eof_owners.insert(handle.run(), handle);
            }
            for run in self.dispatcher.admitted.clone() {
                if self
                    .unsettled_attachment
                    .as_ref()
                    .is_some_and(|handle| handle.client() == Client::Owner && handle.run() == run)
                    || self.pending_eof_owners.contains_key(&run)
                {
                    continue;
                }
                match run::resume_owner(cx, root, self.fence.session, run) {
                    Ok(owner) => {
                        self.pending_eof_owners.insert(run, owner);
                    }
                    Err(_) => {
                        if attachment.is_some() && self.unsettled_attachment.is_none() {
                            self.unsettled_attachment = attachment.take();
                        }
                        self.owner_cleanup_unsettled = true;
                        return Err(Refusal::unknown("original owner attachment is unavailable"));
                    }
                }
            }
        }
        if self.owner_cleanup_unsettled {
            if attachment.is_some() {
                self.unsettled_attachment = attachment.take();
            }
            return Err(Refusal::unknown("attachment cleanup is unsettled"));
        }
        // Every way a connection ends must account for the attachment it opened.
        // A refused or malformed frame used to return before the end-of-stream
        // release, so a second terminal's durable row stayed live and its slot
        // was never reclaimed: eight rejected frames could exhaust the run's
        // secondaries. This client's own row is released now, which for a
        // secondary never stops, detaches or releases the owning client's run. An
        // owning client is deliberately left attached — malformed traffic must
        // not record the stop an owner's EOF means (§8.11), and its row is the
        // run's own, reopened rather than reallocated.
        if let Some(mut handle) = attachment.take() {
            if outcome.is_err() && handle.client() == Client::Secondary && !handle.released() {
                if handle.release(cx, root, Release::Eof).is_err() {
                    self.unsettled_attachment = Some(handle);
                    self.owner_cleanup_unsettled = true;
                    return Err(Refusal::unknown(
                        "attachment disconnect cleanup is unsettled",
                    ));
                }
            } else if outcome.is_err() && !transport_lost && handle.client() == Client::Owner {
                self.unsettled_attachment = Some(handle);
            }
        }
        if state.closed || transport_lost {
            for run in self.dispatcher.admitted.clone() {
                let Some(mut owner) = self.pending_eof_owners.remove(&run) else {
                    self.owner_cleanup_unsettled = true;
                    return Err(Refusal::unknown("original owner attachment is unavailable"));
                };
                if owner.release(cx, root, Release::Eof).is_err() {
                    if owner.pending_release().is_some() {
                        self.unsettled_attachment = Some(owner);
                    } else {
                        self.pending_eof_owners.insert(run, owner);
                    }
                    self.owner_cleanup_unsettled = true;
                    return Err(Refusal::unknown("owner disconnect cleanup is unsettled"));
                }
                self.dispatcher.admitted.remove(&run);
            }
        } else if outcome.is_err() && !self.dispatcher.admitted.is_empty() {
            // Invalid input is not owner EOF, but its disconnected owner must
            // not be silently forgotten or handed to another connection.
            self.owner_cleanup_unsettled = true;
        }
        outcome
    }

    /// The frame loop of one authenticated connection, with the attachment it
    /// opens owned by [`Server::converse`] so every exit accounts for it.
    #[allow(clippy::too_many_arguments)]
    fn converse_frames<T: Transport>(
        &mut self,
        transport: &mut T,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        grant: Grant,
        streams: &mut impl Streams,
        state: &mut Served,
        attachment: &mut Option<Attachment>,
    ) -> Result<(), Refusal> {
        // A connection outlives as many frames as its session needs. Every
        // frame is bounded above, and the transport owns the lifetime; there
        // is no count at which the host would end a live run instead of the
        // client's own end of stream.
        self.pump(transport, streams, attachment.as_ref())?;
        while let Some(body) = read_frame(transport)? {
            let frame = match decode_frame(&body) {
                Ok(frame) => frame,
                // A malformed frame closes the connection before dispatch.
                Err(refusal) => return Err(self.refuse(transport, refusal)),
            };
            match frame {
                Frame::Request(request) => {
                    state.responses += 1;
                    let response =
                        self.answer(cx, observe, root, &grant, &request, attachment, streams);
                    write_frame(transport, &Frame::Response(response))?;
                    if self.owner_cleanup_unsettled {
                        return Err(Refusal::unknown("attachment cleanup is unsettled"));
                    }
                }
                Frame::Stream(stream) => {
                    state.streams += 1;
                    // Terminal input, resize and acknowledgement steer only the
                    // client's own attachment and terminal. Identity, stream
                    // and sequence are checked first, so a refusal has no host
                    // effect: no input reaches a terminal and no acknowledgement
                    // frees a frame.
                    if let Err(refusal) =
                        self.accept_stream(cx, root, &stream, attachment.as_ref(), streams)
                    {
                        return Err(self.refuse(transport, refusal));
                    }
                }
                Frame::Refuse(refusal) => return Err(refusal),
                _ => {
                    return Err(self.refuse(
                        transport,
                        Refusal::invalid(
                            "an authenticated connection carries requests and streams",
                        ),
                    ))
                }
            }
            self.pump(transport, streams, attachment.as_ref())?;
        }
        // End of stream is the client going away: the kernel records the
        // owner's stop or releases this client's own row, and a secondary never
        // stops the owner's run. A detached handle is already gone.
        state.closed = true;
        if let Some(mut handle) = attachment.take() {
            if handle.release(cx, root, Release::Eof).is_err() {
                self.unsettled_attachment = Some(handle);
                self.owner_cleanup_unsettled = true;
                return Err(Refusal::unknown(
                    "attachment disconnect cleanup is unsettled",
                ));
            }
            if handle.client() == Client::Owner {
                self.dispatcher.admitted.remove(&handle.run());
            }
        }
        Ok(())
    }

    /// Validate one inbound stream frame against this connection's actual
    /// attachment and deliver it (052 §8.11).
    ///
    /// A frame must name the attach request that opened this connection's
    /// attachment, a stream this connection carries, and the next sequence for
    /// a terminal frame. Everything is checked before any delivery, so a
    /// foreign request, an unknown stream, a repeated or out-of-order frame or
    /// a malformed payload is refused with no host effect.
    fn accept_stream(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        frame: &StreamFrame,
        attachment: Option<&Attachment>,
        streams: &mut impl Streams,
    ) -> Result<(), Refusal> {
        let Some(attachment) = attachment else {
            return Err(Refusal::invalid(
                "a stream frame names the client's own attachment",
            ));
        };
        if Some(frame.id) != self.attach {
            return Err(Refusal::invalid(
                "a stream frame names this connection's own attach request",
            ));
        }
        // Holding the value is not authority: the row can have been released,
        // replaced, or ended by its run's closure since the attach. The row is
        // re-read now, and the owner receives the handle so it can fence the
        // delivery on the same lifetime immediately before it writes.
        match attachment.attached(cx, root) {
            Ok(true) => {}
            Ok(false) => {
                return Err(Refusal::invalid(
                    "this connection's attachment is no longer attached",
                ))
            }
            Err(error) => return Err(refusal_from(error)),
        }
        match frame.kind {
            StreamKind::Ack => {
                // An acknowledgement is only its sequence: no payload and no
                // size fields. A malformed one refuses before the window moves.
                if frame.payload.is_some() || frame.rows.is_some() || frame.columns.is_some() {
                    return Err(Refusal::invalid(
                        "an acknowledgement carries only the sequence it consumed",
                    ));
                }
                // The sequence must name a frame this connection published and
                // has not freed. A duplicate, a stale one, one for a control
                // frame and one for a retained position this connection never
                // published all refuse, so none of them frees the window.
                if !self.outbound.acknowledge(frame.stream_id, frame.seq) {
                    return Err(Refusal::invalid(
                        "a client acknowledges no open frame this connection published",
                    ));
                }
                Ok(())
            }
            StreamKind::TerminalInput => {
                if frame.stream_id != TERMINAL_STREAM {
                    return Err(Refusal::invalid(
                        "terminal input names the attached run's terminal stream",
                    ));
                }
                if frame.rows.is_some() || frame.columns.is_some() {
                    return Err(Refusal::invalid("terminal input carries no terminal size"));
                }
                let Some(payload) = frame.payload.as_deref() else {
                    return Err(Refusal::invalid("terminal input carries bytes"));
                };
                let bytes = decode_stream_payload(payload)?;
                self.next_inbound(frame)?;
                // Delivered once, and only after the whole frame checked out.
                // An owner with no terminal refuses, which closes this
                // connection rather than dropping the keystroke.
                streams.input(attachment, &bytes)
            }
            StreamKind::Resize => {
                if frame.stream_id != TERMINAL_STREAM {
                    return Err(Refusal::invalid(
                        "a resize names the attached run's terminal stream",
                    ));
                }
                if frame.payload.is_some() {
                    return Err(Refusal::invalid("a resize frame carries no payload"));
                }
                let (Some(rows), Some(columns)) = (frame.rows, frame.columns) else {
                    return Err(Refusal::invalid(
                        "a resize frame carries positive rows and columns",
                    ));
                };
                if rows == 0 || columns == 0 {
                    return Err(Refusal::invalid(
                        "a resize frame carries positive rows and columns",
                    ));
                }
                self.next_inbound(frame)?;
                // The owner keeps the newest size. A host with no terminal
                // refuses instead of reporting a resize it never applied.
                streams.resize(attachment, TerminalSize { rows, columns })
            }
            _ => Err(Refusal::invalid(
                "a client sends terminal input, resize or acknowledgement",
            )),
        }
    }

    /// Take the next inbound sequence on `stream_id`: exactly the next one, so
    /// repeated or out-of-order terminal input is never replayed.
    fn next_inbound(&mut self, frame: &StreamFrame) -> Result<(), Refusal> {
        let expected = self.inbound.entry(frame.stream_id).or_insert(0);
        if frame.seq != *expected {
            return Err(Refusal::invalid(
                "terminal input and resize occupy the stream's next sequence",
            ));
        }
        *expected = expected.saturating_add(1);
        Ok(())
    }

    /// Publish captured output as data frames, stopping at the client's
    /// acknowledgement window. A chunk that does not fit is kept, so the next
    /// acknowledgement resumes it in order; control frames never wait behind it
    /// (§8.11).
    fn pump<T: Transport>(
        &mut self,
        transport: &mut T,
        streams: &mut impl Streams,
        attachment: Option<&Attachment>,
    ) -> Result<(), Refusal> {
        let Some(id) = self.attach else {
            // Nothing is attached, so there is nothing to publish.
            return Ok(());
        };
        loop {
            if self.carry.is_none() {
                self.carry = streams.next_chunk(attachment);
            }
            let Some(chunk) = self.carry.take() else {
                return Ok(());
            };
            // A retained chunk carries the absolute log position of its first
            // byte, so the frame sequence is that position: a client's cursor,
            // a gap and the data around it are one series across reconnects and
            // chunk boundaries. A gap the log reports this way becomes one
            // `gap` frame naming the first missing position, ahead of the data.
            let left = match chunk.cursor {
                Some(from) => self
                    .outbound
                    .push_retained(id, chunk.stream_id, chunk.kind, from, &chunk.bytes)
                    .map(|left| left.to_vec())?,
                None => self
                    .outbound
                    .push_chunk(id, chunk.stream_id, chunk.kind, &chunk.bytes)
                    .to_vec(),
            };
            for frame in self.outbound.take() {
                write_frame(transport, &Frame::Stream(frame))?;
            }
            if !left.is_empty() {
                self.carry = Some(StreamChunk {
                    stream_id: chunk.stream_id,
                    kind: chunk.kind,
                    cursor: chunk
                        .cursor
                        .map(|from| from + (chunk.bytes.len() - left.len()) as u64),
                    bytes: left,
                });
                return Ok(());
            }
        }
    }

    /// One request: fence, grant, then the typed entry point.
    #[allow(clippy::too_many_arguments)]
    fn answer(
        &mut self,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        grant: &Grant,
        request: &Request,
        attachment: &mut Option<Attachment>,
        streams: &mut impl Streams,
    ) -> Response {
        let answered = (|| -> Result<ResponseOutcome, Refusal> {
            // The grant's binding is the host's own record; the request's
            // claims are checked against it afterwards.
            self.fence.verify_grant(grant.binding())?;
            self.fence.verify(request)?;
            grant.authorise(request.method(), request.body.target())?;
            if request.method().mutates() {
                cx.require(crate::ctx::Capability::HostAuthority)
                    .map_err(|_| {
                        Refusal::forbidden("a guest placement cannot dispatch a host effect")
                    })?;
            }
            let digest = request.digest()?;
            let session = grant.session();
            match &request.body {
                RequestBody::Observe(body) => {
                    let value = self.dispatcher.observe(observe, body)?;
                    Ok(ResponseOutcome::Observed(value))
                }
                RequestBody::Invoke(body) => {
                    let reply = self
                        .dispatcher
                        .invoke(cx, observe, root, session, request.id, digest, body)?;
                    Ok(match reply {
                        AdmissionReply::Accepted(accepted) => ResponseOutcome::Accepted(accepted),
                        AdmissionReply::Completed(completed) => {
                            ResponseOutcome::Completed(completed)
                        }
                    })
                }
                RequestBody::Attach(body) => {
                    // One attachment per connection. Attaching to another run
                    // refuses, and replacing this connection's own secondary
                    // releases the old row first, so nothing is orphaned.
                    if let Some(current) = attachment.as_mut() {
                        if current.run() != body.run {
                            return Err(Refusal::forbidden(
                                "this connection is attached to another run; detach it first",
                            ));
                        }
                    }
                    // The requested cursor meets the retained reader before
                    // anything opens, for every stream this connection carries.
                    // Negotiation is pure: a source that cannot serve the cursor
                    // refuses the attach, and a replacement the host then
                    // refuses has moved nothing, so the stream this connection
                    // is still serving keeps its position (§8.11).
                    let cursor = body.cursor.unwrap_or(0);
                    let Some(start) = streams.resume(body.run, &body.positions()) else {
                        return Err(Refusal::invalid(
                            "this attachment cannot resume at the requested cursor",
                        ));
                    };
                    let handle = self
                        .dispatcher
                        .attach(cx, root, session, request.id, body)?;
                    let view = AttachView {
                        attachment: handle.id(),
                        run: handle.run(),
                        client: handle.client(),
                        cursor,
                    };
                    if let Some(previous) = attachment.take() {
                        // A different attachment id on the same run is a second
                        // row this connection is leaving: release it as an end
                        // of stream, which for a secondary never stops the run.
                        if previous.id() != handle.id() && previous.client() == Client::Secondary {
                            let mut previous = previous;
                            if previous.release(cx, root, Release::Eof).is_err() {
                                self.unsettled_attachment = Some(previous);
                                self.unsettled_replacement = Some(handle);
                                self.owner_cleanup_unsettled = true;
                                return Err(Refusal::unknown(
                                    "attachment replacement cleanup is unsettled",
                                ));
                            }
                        }
                    }
                    // A replacement is a new attachment lifetime, and it is
                    // rebound in one step: the previous attachment's window,
                    // held chunk, numbering and inbound sequence are dropped, so
                    // its unacknowledged frames cannot be freed by this
                    // attachment's acknowledgements and its cursor cannot be
                    // measured against the previous numbering. The requested
                    // cursor then positions this attachment's own stream.
                    self.outbound = Outbound::new();
                    self.carry = None;
                    self.inbound.clear();
                    // Every negotiated stream is numbered from its own
                    // position, so a second retained stream resumes where its
                    // log does instead of replaying from zero or inventing a
                    // gap.
                    for (stream_id, position) in &start {
                        self.outbound.resume(*stream_id, *position);
                    }
                    // The negotiated positions are committed only now, for the
                    // attachment that actually opened.
                    streams.start(&handle, &start);
                    self.attach = Some(request.id);
                    *attachment = Some(handle);
                    Ok(ResponseOutcome::Observed(
                        serde_json::to_value(view)
                            .map_err(|_| Refusal::invalid("an attachment view is not encodable"))?,
                    ))
                }
                RequestBody::Cancel(body) => {
                    self.dispatcher
                        .cancel(cx, root, session, attachment.as_ref(), body)?;
                    Ok(ResponseOutcome::Observed(Value::Null))
                }
                RequestBody::Detach(body) => {
                    // An acknowledged detach releases this client's own
                    // lifetime and records no stop, so a Detached run keeps
                    // going (§8.11). The handle is taken, so a later end of
                    // stream has nothing left to send a stop for.
                    if attachment
                        .as_ref()
                        .is_some_and(|handle| handle.run() != body.run)
                    {
                        return Err(Refusal::forbidden(
                            "this connection is attached to another run",
                        ));
                    }
                    match attachment.as_mut() {
                        Some(handle) => {
                            let is_owner = handle.client() == Client::Owner;
                            if let Err(error) = handle.release(cx, root, Release::Detach) {
                                if !matches!(error, run::RunError::Refused(_)) {
                                    self.owner_cleanup_unsettled = true;
                                    return Err(Refusal::unknown("attachment detach is unsettled"));
                                }
                                return Err(refusal_from(error));
                            }
                            if is_owner {
                                self.dispatcher.admitted.remove(&body.run);
                            }
                            attachment.take();
                        }
                        None => {
                            if !self.dispatcher.admitted.contains(&body.run) {
                                return Err(Refusal::forbidden(
                                    "this connection did not admit the run",
                                ));
                            }
                            let mut owner = run::resume_owner(cx, root, session, body.run)
                                .map_err(refusal_from)?;
                            match owner.release(cx, root, Release::Detach) {
                                Ok(()) => {
                                    self.dispatcher.admitted.remove(&body.run);
                                }
                                Err(error) => {
                                    if !matches!(error, run::RunError::Refused(_)) {
                                        self.unsettled_attachment = Some(owner);
                                        self.owner_cleanup_unsettled = true;
                                        return Err(Refusal::unknown(
                                            "attachment detach is unsettled",
                                        ));
                                    }
                                    return Err(refusal_from(error));
                                }
                            }
                        }
                    }
                    self.attach = None;
                    // A detached connection has no attachment to publish for,
                    // so an unpublished chunk from it is dropped rather than
                    // carried to whatever attaches next, and its terminal
                    // accepts no further input.
                    self.carry = None;
                    self.inbound.clear();
                    Ok(ResponseOutcome::Observed(Value::Null))
                }
            }
        })();
        match answered {
            Ok(outcome) => Response {
                id: request.id,
                outcome,
            },
            Err(refusal) => Response::refused(request.id, refusal),
        }
    }

    /// Write a connection-level refusal once and keep its code.
    fn refuse<T: Transport>(&self, transport: &mut T, refusal: Refusal) -> Refusal {
        let _ = write_frame(transport, &Frame::Refuse(refusal.clone()));
        refusal
    }
}

/// What an `attach` request opened, as the host reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachView {
    pub attachment: AttachmentId,
    pub run: RunId,
    pub client: Client,
    /// The cursor the client asked to resume from; a first read reports a gap
    /// when rotation removed those bytes (§8.11).
    pub cursor: u64,
}

/// The operation set a workspace session's capability normally carries: every
/// typed entry point the channel serves. A caller may narrow it.
pub fn session_operations() -> BTreeSet<Method> {
    [
        Method::Observe,
        Method::Invoke,
        Method::Attach,
        Method::Cancel,
        Method::Detach,
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests;
