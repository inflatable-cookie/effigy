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
use std::collections::{BTreeMap, BTreeSet};
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
/// Longest diagnostic a refusal carries: a sentence, never a transcript.
pub const MAX_DIAGNOSTIC_BYTES: usize = 512;
/// Frames one connection may carry before it is closed. A bounded session, so
/// a connection cannot outlive its own handshake by forever.
pub const MAX_FRAMES_PER_CONNECTION: usize = 4096;

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
}
impl ObserveBody {
    fn check(&self) -> Result<(), Refusal> {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachBody {
    pub run: RunId,
    pub streams: Vec<StreamKind>,
    pub cursor: Option<u64>,
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
        if self.cursor.is_some_and(|cursor| cursor > (1 << 40)) {
            return Err(Refusal::invalid("a stream cursor is bounded"));
        }
        Ok(())
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
        let binding = GrantBinding {
            host_user: enrolment.host_user.clone(),
            checkout: enrolment.checkout.clone(),
            session: Session {
                run: request.session_run_id,
                generation: request.session_generation,
            },
            environment_generation: request.environment_generation,
            container_id: request.container_id.clone(),
            vm_boot_id: request.vm_boot_id.clone(),
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
    /// End every grant bound to a session run at or below a generation. A
    /// capability from a dead generation is worth nothing (§8.11).
    pub fn revoke_generation(&mut self, session_run_id: RunId, generation: Generation) {
        for grant in &mut self.grants {
            if grant.binding.session.run == session_run_id
                && grant.binding.session.generation <= generation
            {
                grant.revoked = true;
            }
        }
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

/// A stream frame. `seq` is monotonic per stream.
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
impl StreamFrame {
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
    sizes: std::collections::VecDeque<usize>,
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
    /// Offer one data frame of `decoded` bytes.
    pub fn offer(&mut self, decoded: usize) -> Queue {
        if decoded > MAX_STREAM_PAYLOAD
            || self.unacked + 1 > MAX_UNACKED_STREAM_FRAMES
            || self.queued + decoded > MAX_QUEUED_STREAM_BYTES
        {
            return Queue::Backpressure;
        }
        self.unacked += 1;
        self.queued += decoded;
        self.sizes.push_back(decoded);
        Queue::Admitted
    }
    /// The receiver consumed `count` frames, which frees their queued bytes.
    pub fn acknowledge(&mut self, count: usize) {
        for _ in 0..count {
            let Some(freed) = self.sizes.pop_front() else {
                break;
            };
            self.unacked = self.unacked.saturating_sub(1);
            self.queued = self.queued.saturating_sub(freed);
        }
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
    last_seq: BTreeMap<(u32, StreamKind), u64>,
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
        let mut offset = 0;
        while offset < bytes.len() {
            let end = (offset + MAX_STREAM_PAYLOAD).min(bytes.len());
            if self.window.offer(end - offset) == Queue::Backpressure {
                break;
            }
            let seq = self.sequence(stream_id, kind);
            self.data.push(StreamFrame {
                id,
                stream_id,
                seq,
                kind,
                payload: Some(encode_stream_payload(&bytes[offset..end])),
                rows: None,
                columns: None,
            });
            offset = end;
        }
        &bytes[offset..]
    }
    fn sequence(&mut self, stream_id: u32, kind: StreamKind) -> u64 {
        let next = self
            .last_seq
            .entry((stream_id, kind))
            .and_modify(|seq| *seq += 1)
            .or_insert(0);
        *next
    }
    /// A control frame is never subject to the data window (§8.11).
    pub fn control_reserved(&self) -> bool {
        true
    }
    pub fn take(&mut self) -> Vec<StreamFrame> {
        std::mem::take(&mut self.data)
    }
    pub fn window(&self) -> &StreamWindow {
        &self.window
    }
    pub fn window_mut(&mut self) -> &mut StreamWindow {
        &mut self.window
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
        let reply = match run::reserve(cx, root, key, digest, run).map_err(refusal_from)? {
            Reservation::Recorded(recovered) => replay(recovered)?,
            Reservation::Fresh(ticket) => {
                // A refused admission keeps the reservation recorded against
                // this key, so a later retry reads `unknown` instead of
                // admitting a second run under a fresh id.
                self.admission.admit(cx, &ticket, &resolved)?;
                AdmissionReply::Accepted(Accepted {
                    run: ticket.run(),
                    placement: Placement::Host,
                })
            }
        };
        if let AdmissionReply::Accepted(accepted) = &reply {
            self.admitted.insert(accepted.run);
        }
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
        let handle = run::resume_owner(cx, root, session, run).map_err(refusal_from)?;
        self.admitted.insert(run);
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
        let id = self.ids.next()?;
        match handle {
            Some(handle) if handle.run() == body.run => {
                handle.control(cx, root, id, RequestAction::Stop)
            }
            _ => {
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
        }
    }
    pub fn grants(&self) -> &GrantBook {
        &self.grants
    }
    pub fn grants_mut(&mut self) -> &mut GrantBook {
        &mut self.grants
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
    ) -> Result<Served, Refusal> {
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
                self.converse(transport, cx, observe, root, grant, &mut state)?;
                Ok(state)
            }
            None => Ok(Served {
                responses: 0,
                streams: 0,
                closed: true,
            }),
        }
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
    fn converse<T: Transport>(
        &mut self,
        transport: &mut T,
        cx: &EffectContext,
        observe: &ObserveContext,
        root: &StateRoot,
        grant: Grant,
        state: &mut Served,
    ) -> Result<(), Refusal> {
        let mut attachment: Option<Attachment> = None;
        let mut detach_acknowledged = false;
        let mut frames = 0usize;
        while frames < MAX_FRAMES_PER_CONNECTION {
            frames += 1;
            let Some(body) = read_frame(transport)? else {
                break;
            };
            let frame = match decode_frame(&body) {
                Ok(frame) => frame,
                // A malformed frame closes the connection before dispatch.
                Err(refusal) => return Err(self.refuse(transport, refusal)),
            };
            match frame {
                Frame::Request(request) => {
                    state.responses += 1;
                    let response = self.answer(
                        cx,
                        observe,
                        root,
                        &grant,
                        &request,
                        &mut attachment,
                        &mut detach_acknowledged,
                    );
                    write_frame(transport, &Frame::Response(response))?;
                }
                Frame::Stream(stream) => {
                    state.streams += 1;
                    // Terminal input and acknowledgement frames only steer the
                    // client's own attachment; unknown streams refuse before an
                    // effect.
                    if attachment.is_none() {
                        let refusal =
                            Refusal::invalid("a stream frame names the client's own attachment");
                        return Err(self.refuse(transport, refusal));
                    }
                    // Terminal input, resize and acknowledgement steer the
                    // client's own terminal, never the host authority. They
                    // create no effect here, and the bounded stream loop that
                    // consumes them belongs to the attachment's owner.
                    if !matches!(
                        stream.kind,
                        StreamKind::Ack | StreamKind::TerminalInput | StreamKind::Resize
                    ) {
                        return Err(self.refuse(
                            transport,
                            Refusal::invalid(
                                "a client sends terminal input, resize or acknowledgement",
                            ),
                        ));
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
        }
        if let Some(mut handle) = attachment {
            if !detach_acknowledged {
                // EOF without a detach: the kernel records the owner's stop or
                // releases this client's own row, and a secondary never stops
                // the owner's run.
                handle
                    .release(cx, root, Release::Eof)
                    .map_err(refusal_from)?;
            }
        }
        state.closed = true;
        Ok(())
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
        detach_acknowledged: &mut bool,
    ) -> Response {
        let answered = (|| -> Result<ResponseOutcome, Refusal> {
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
                    let handle = self
                        .dispatcher
                        .attach(cx, root, session, request.id, body)?;
                    let view = AttachView {
                        attachment: handle.id(),
                        run: handle.run(),
                        client: handle.client(),
                        cursor: body.cursor.unwrap_or(0),
                    };
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
                    // going (§8.11).
                    match attachment
                        .as_mut()
                        .filter(|handle| handle.run() == body.run)
                    {
                        Some(handle) => handle
                            .release(cx, root, Release::Detach)
                            .map_err(refusal_from)?,
                        None => self.dispatcher.detach(cx, root, session, body)?,
                    }
                    *detach_acknowledged = true;
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
