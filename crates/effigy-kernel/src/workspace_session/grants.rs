//! Memory-only session capabilities, enrolment and durable terminal records
//! (§8.3, §8.8, §8.11).
//!
//! The capability is session state of the session supervisor: it lives in this
//! authority and nowhere else, is never persisted and is never logged. It is
//! bound to the host user, the checkout, the session run and its generation,
//! the environment generation, the full container identity and the VM boot id,
//! and to the operations and targets it may use.
//!
//! Enrolment is a separate, versioned credential: a direct terminal presents it
//! and receives a capability for enrolment only. A new environment generation
//! revokes both.
//!
//! Terminal attachments are recorded durably, one row each, so a supervisor
//! restart reads who was attached and a client's death is recorded as `lost`
//! without touching the session. A secondary terminal never holds the run's
//! owning pipe, so nothing it does — including disconnecting — can stop the
//! session.

use super::*;
use crate::ctx::StopCause;
use crate::human::Category;
use crate::store::Store;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// The additive table this module owns (§8.7 terminal enrolment records).
const TABLE: &str = "workspace_session_terminals";
/// The additive table that records which terminal admitted which run.
const OWNED_TABLE: &str = "workspace_session_owned_runs";
/// Bytes in a capability or an enrolment secret.
pub const SECRET_BYTES: usize = 32;
/// Bytes in a terminal id.
pub const TERMINAL_ID_BYTES: usize = 16;
const TEXT_MAX: usize = 512;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Reads host entropy from `/dev/urandom`. A capability is unguessable or it is
/// not issued: there is no clock or pid fallback.
pub fn host_entropy(bytes: usize) -> Result<Vec<u8>, Refusal> {
    let mut buffer = vec![0u8; bytes];
    let mut file = std::fs::File::open("/dev/urandom").map_err(|error| {
        Refusal::new(RefusalCode::Unavailable, format!("/dev/urandom ({error})"))
    })?;
    file.read_exact(&mut buffer).map_err(|error| {
        Refusal::new(RefusalCode::Unavailable, format!("/dev/urandom ({error})"))
    })?;
    Ok(buffer)
}

fn text(value: &str, what: &str) -> Result<String, Refusal> {
    if value.is_empty() || value.len() > TEXT_MAX {
        return Err(Refusal::new(
            RefusalCode::InvalidRequest,
            format!("a session {what} is out of bounds"),
        ));
    }
    Ok(value.to_owned())
}

/// Constant-time equality: a secret is compared without an early exit.
fn equal_secret(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in left.iter().zip(right) {
        difference |= left ^ right;
    }
    difference == 0
}

/// Which front end opened a terminal. All three report the same managed
/// session identity; they differ only in how they render it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    Tui,
    Text,
    Headless,
}
impl TerminalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Text => "text",
            Self::Headless => "headless",
        }
    }
    fn parse(value: &str) -> Result<Self, Refusal> {
        match value {
            "tui" => Ok(Self::Tui),
            "text" => Ok(Self::Text),
            "headless" => Ok(Self::Headless),
            other => Err(Refusal::new(
                RefusalCode::InvalidRequest,
                format!("a terminal kind `{other}`"),
            )),
        }
    }
}

/// One terminal's durable id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TerminalId(String);
impl TerminalId {
    pub fn generate(entropy: [u8; TERMINAL_ID_BYTES]) -> Self {
        Self(hex(&entropy))
    }
    pub fn parse(value: &str) -> Result<Self, Refusal> {
        if value.len() != TERMINAL_ID_BYTES * 2
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Refusal::new(
                RefusalCode::InvalidRequest,
                "a terminal id is 32 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Display for TerminalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Everything a capability is bound to. The supervisor keeps its own copy and
/// compares it with every request's claim; a request never proves these facts
/// about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBinding {
    user: String,
    checkout: String,
    session: SessionRef,
    container: String,
    boot: String,
}
impl SessionBinding {
    pub fn new(
        user: &str,
        checkout: &str,
        session: SessionRef,
        container: &str,
        boot: &str,
    ) -> Result<Self, Refusal> {
        Ok(Self {
            user: text(user, "user")?,
            checkout: text(checkout, "checkout")?,
            session,
            container: text(container, "container identity")?,
            boot: text(boot, "boot identity")?,
        })
    }
    pub fn user(&self) -> &str {
        &self.user
    }
    pub fn checkout(&self) -> &str {
        &self.checkout
    }
    pub fn session(&self) -> &SessionRef {
        &self.session
    }
    pub fn container(&self) -> &str {
        &self.container
    }
    pub fn boot(&self) -> &str {
        &self.boot
    }
    /// Whether a claim matches this binding in every field.
    pub fn matches(&self, claim: &SessionBinding) -> bool {
        self == claim
    }
}

/// The enrolment credential: versioned, bound to the environment generation and
/// the container identity, and usable only to enrol (§8.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enrolment {
    version: u32,
    environment_generation: Generation,
    container: String,
    boot: String,
    secret: Vec<u8>,
}
impl Enrolment {
    pub const VERSION: u32 = 1;
    pub fn new(
        environment_generation: Generation,
        container: &str,
        boot: &str,
        secret: Vec<u8>,
    ) -> Result<Self, Refusal> {
        if secret.len() != SECRET_BYTES {
            return Err(Refusal::new(
                RefusalCode::InvalidRequest,
                "an enrolment secret is 32 bytes",
            ));
        }
        Ok(Self {
            version: Self::VERSION,
            environment_generation,
            container: text(container, "container identity")?,
            boot: text(boot, "boot identity")?,
            secret,
        })
    }
    pub fn version(&self) -> u32 {
        self.version
    }
    pub fn environment_generation(&self) -> Generation {
        self.environment_generation
    }
    pub fn container(&self) -> &str {
        &self.container
    }
    pub fn boot(&self) -> &str {
        &self.boot
    }
}

/// The session capability. It is unguessable, memory-only and never printed.
#[derive(Clone, PartialEq, Eq)]
pub struct Capability(String);
impl Capability {
    pub fn issue(entropy: &[u8]) -> Result<Self, Refusal> {
        if entropy.len() != SECRET_BYTES {
            return Err(Refusal::new(
                RefusalCode::Unavailable,
                "a capability needs 32 bytes of entropy",
            ));
        }
        Ok(Self(hex(entropy)))
    }
    /// Hands the capability to the issuing supervisor, which places it in
    /// guest-private storage. Nothing else may read it.
    pub fn expose(&self) -> &str {
        &self.0
    }
    fn matches(&self, presented: &Capability) -> bool {
        equal_secret(self.0.as_bytes(), presented.0.as_bytes())
    }
}
impl std::fmt::Debug for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Capability([REDACTED])")
    }
}

/// A grant held by one authenticated client. It is independent of the
/// supervisor's lifetime: a client can hold it across a reconnect, and a
/// reconnect re-authenticates and reads a fresh grant.
#[derive(Debug, Clone)]
pub struct ClientGrant {
    binding: SessionBinding,
    operations: Vec<Method>,
}
impl ClientGrant {
    pub fn allows(&self, method: Method) -> bool {
        self.operations.contains(&method)
    }
    /// Refuses an operation outside the grant's scope.
    pub fn authorise(&self, method: Method) -> Result<(), Refusal> {
        if self.allows(method) {
            Ok(())
        } else {
            Err(Refusal::new(
                RefusalCode::Forbidden,
                format!("this session grant does not allow `{}`", method.as_str()),
            ))
        }
    }
    /// A client cannot cancel or detach another client's run merely by naming
    /// its id: only the session and its own child runs are in scope.
    pub fn authorise_target(
        &self,
        reader: &StoreReader,
        session: &SessionRef,
        target: RunId,
    ) -> Result<(), RunError> {
        if &self.binding.session != session {
            return Err(RunError::refused("this grant belongs to another session"));
        }
        if target == session.run {
            return Ok(());
        }
        match run::summary(reader, target) {
            Observed::Verified(summary, _) if summary.parent == Some(session.run) => Ok(()),
            Observed::Verified(_, _) => Err(RunError::refused(
                "a client cannot control another client's run merely by naming its id",
            )),
            Observed::Absent(_) => Err(RunError::refused(
                "the requested run is not in this journal",
            )),
            Observed::Unknown(reason) => Err(RunError::unavailable(reason.to_string())),
        }
    }
}

/// The terminal states §8.11 distinguishes. A detached or lost terminal leaves
/// the session running; only the session's own settle stops it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Attached,
    Detached,
    Lost,
    Stopped,
}
impl TerminalState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Attached => "attached",
            Self::Detached => "detached",
            Self::Lost => "lost",
            Self::Stopped => "stopped",
        }
    }
}

/// How one terminal's ownership of its work ended (§8.11): EOF without an
/// acknowledged detach, the client going away, or a signal the terminal
/// received. None of these stops the session; they stop the runs that terminal
/// admitted, and an acknowledged detach stops nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerLoss {
    /// End of input on the terminal, with no detach acknowledged.
    EndOfInput,
    /// The terminal or its client disappeared without closing cleanly.
    Disconnected,
    /// The terminal received this signal: TERM, HUP, or an operator's own.
    Signal(i32),
}
impl OwnerLoss {
    /// The cause the owned work is stopped with. A signal names itself; an EOF
    /// or a disconnect is owner loss.
    pub fn cause(self) -> StopCause {
        match self {
            Self::EndOfInput | Self::Disconnected => StopCause::OwnerLost,
            Self::Signal(signal) => StopCause::Signal(signal),
        }
    }
}

/// What ending one terminal did. `owned` is exactly the work that terminal
/// admitted and that has not settled, so the supervisor stops that list and
/// nothing else; the session run is never in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEnd {
    terminal: TerminalId,
    kind: TerminalKind,
    /// True when the terminal had acknowledged a detach: its work stays.
    released: bool,
    cause: StopCause,
    owned: Vec<RunId>,
}
impl TerminalEnd {
    pub fn terminal(&self) -> &TerminalId {
        &self.terminal
    }
    pub fn kind(&self) -> TerminalKind {
        self.kind
    }
    /// Whether the terminal had already detached, so nothing is stopped.
    pub fn released(&self) -> bool {
        self.released
    }
    pub fn cause(&self) -> StopCause {
        self.cause.clone()
    }
    pub fn owned(&self) -> &[RunId] {
        &self.owned
    }
}

/// One live terminal handle. Its `Drop` marks the handle gone in memory; the
/// supervisor's next turn records it as `lost` through
/// [`SessionAuthority::reap`]. Dropping it never writes a stop request.
#[derive(Debug)]
pub struct AttachedTerminal {
    id: TerminalId,
    session: SessionRef,
    kind: TerminalKind,
    capability: Option<Capability>,
    dropped: Arc<AtomicBool>,
    released: bool,
}
impl AttachedTerminal {
    pub fn id(&self) -> &TerminalId {
        &self.id
    }
    pub fn kind(&self) -> TerminalKind {
        self.kind
    }
    /// The one session identity every kind of front end reports.
    pub fn identity(&self) -> &SessionRef {
        &self.session
    }
    /// The capability the first terminal opened through Effigy receives as part
    /// of its entry (§8.3). Every later terminal receives `None`.
    pub fn session_capability(&self) -> Option<&Capability> {
        self.capability.as_ref()
    }
    pub fn dropped(&self) -> bool {
        self.dropped.load(Ordering::Acquire)
    }
}
impl Drop for AttachedTerminal {
    fn drop(&mut self) {
        if !self.released {
            self.dropped.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug)]
struct TerminalSlot {
    kind: TerminalKind,
    state: TerminalState,
    capability: bool,
    dropped: Arc<AtomicBool>,
}

/// The supervisor's memory-only session authority: the binding, the enrolment
/// grant, the capability and the live terminals.
#[derive(Debug)]
pub struct SessionAuthority {
    binding: SessionBinding,
    enrolment: Enrolment,
    capability: Option<Capability>,
    operations: Vec<Method>,
    terminals: BTreeMap<TerminalId, TerminalSlot>,
}

impl SessionAuthority {
    /// Opens the authority for one session. Enrolment must already name this
    /// session's environment generation and container identity.
    pub fn open(binding: SessionBinding, enrolment: Enrolment) -> Result<Self, Refusal> {
        if enrolment.environment_generation() != binding.session().environment_generation()
            || enrolment.container() != binding.container()
            || enrolment.boot() != binding.boot()
        {
            return Err(Refusal::new(
                RefusalCode::IdentityMismatch,
                "the enrolment credential names another environment generation or container",
            ));
        }
        Ok(Self {
            binding,
            enrolment,
            capability: None,
            operations: vec![
                Method::Observe,
                Method::Invoke,
                Method::Attach,
                Method::Cancel,
                Method::Detach,
            ],
            terminals: BTreeMap::new(),
        })
    }

    pub fn session(&self) -> &SessionRef {
        &self.binding.session
    }
    pub fn capability(&self) -> Option<&Capability> {
        self.capability.as_ref()
    }

    /// Enrols a direct terminal: it presents the credential, the supervisor
    /// checks the environment generation and the container identity, and the
    /// session issues (or re-issues) its capability. Enrolment cannot execute
    /// another operation.
    pub fn enrol(&mut self, presented: &Enrolment) -> Result<Capability, Refusal> {
        if presented.version() != Enrolment::VERSION {
            return Err(Refusal::new(
                RefusalCode::Unauthenticated,
                "the enrolment credential was written by another Effigy version",
            ));
        }
        if presented.environment_generation() != self.enrolment.environment_generation() {
            return Err(Refusal::new(
                RefusalCode::StaleContext,
                "a new environment generation revokes enrolment; re-prepare the workspace",
            ));
        }
        if presented.container() != self.enrolment.container()
            || presented.boot() != self.enrolment.boot()
        {
            return Err(Refusal::new(
                RefusalCode::IdentityMismatch,
                "the enrolment credential names another container",
            ));
        }
        if !equal_secret(&presented.secret, &self.enrolment.secret) {
            return Err(Refusal::new(
                RefusalCode::Unauthenticated,
                "the enrolment credential does not match",
            ));
        }
        let capability = match &self.capability {
            Some(capability) => capability.clone(),
            None => {
                let capability = Capability::issue(&host_entropy(SECRET_BYTES)?)?;
                self.capability = Some(capability.clone());
                capability
            }
        };
        Ok(capability)
    }

    /// Authenticates a capability and returns that client's own grant. This is
    /// read-only: authenticating with an existing capability writes nothing.
    pub fn authenticate(&mut self, presented: &Capability) -> Result<ClientGrant, Refusal> {
        let Some(capability) = &self.capability else {
            return Err(Refusal::new(
                RefusalCode::Unauthenticated,
                "this session has issued no capability",
            ));
        };
        if !capability.matches(presented) {
            return Err(Refusal::new(
                RefusalCode::Unauthenticated,
                "the session capability does not match",
            ));
        }
        Ok(ClientGrant {
            binding: self.binding.clone(),
            operations: self.operations.clone(),
        })
    }

    /// Revokes every grant when a session observes a new environment
    /// generation. Nothing survives it, and the session is not re-issued.
    pub fn revoke_on_generation(&mut self, environment_generation: Generation) -> bool {
        if environment_generation == self.binding.session().environment_generation() {
            return false;
        }
        self.capability = None;
        for slot in self.terminals.values_mut() {
            if slot.state == TerminalState::Attached {
                slot.state = TerminalState::Stopped;
            }
        }
        true
    }

    /// Attaches one terminal to the session and records it. The first terminal
    /// opened through Effigy receives the capability; every later one attaches
    /// to the same run without one.
    pub fn attach(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        grant: &ClientGrant,
        kind: TerminalKind,
        terminal: TerminalId,
        cursor: Option<StreamCursor>,
    ) -> Result<AttachedTerminal, Refusal> {
        if !self.binding.matches(&grant.binding) {
            return Err(Refusal::new(
                RefusalCode::IdentityMismatch,
                "this grant names another session",
            ));
        }
        grant.authorise(Method::Attach)?;
        if self.terminals.contains_key(&terminal) {
            return Err(Refusal::new(
                RefusalCode::InvalidRequest,
                "this terminal id is already attached",
            ));
        }
        // The first terminal opened through Effigy receives the session
        // capability as part of its entry; every later terminal never sees it.
        // A capability the entry already issued is handed over here rather than
        // re-issued, so one session keeps one credential.
        let first = self.terminals.is_empty();
        if first && self.capability.is_none() {
            let capability = Capability::issue(&host_entropy(SECRET_BYTES)?)?;
            self.capability = Some(capability);
        }
        let capability = if first { self.capability.clone() } else { None };
        let session = self.binding.session().clone();
        write_terminal(
            cx,
            root,
            &session,
            &terminal,
            kind,
            TerminalState::Attached,
            cursor,
            first,
        )?;
        let dropped = Arc::new(AtomicBool::new(false));
        self.terminals.insert(
            terminal.clone(),
            TerminalSlot {
                kind,
                state: TerminalState::Attached,
                capability: first,
                dropped: dropped.clone(),
            },
        );
        Ok(AttachedTerminal {
            id: terminal,
            session,
            kind,
            capability,
            dropped,
            released: false,
        })
    }

    /// Releases one terminal explicitly. The session keeps running: a detach
    /// releases the client, never a `Detached` run.
    pub fn detach(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        terminal: &mut AttachedTerminal,
        cursor: Option<StreamCursor>,
    ) -> Result<(), Refusal> {
        let Some(slot) = self.terminals.get_mut(&terminal.id) else {
            return Err(Refusal::new(
                RefusalCode::InvalidRequest,
                "this terminal is not attached to the session",
            ));
        };
        write_terminal(
            cx,
            root,
            &self.binding.session,
            &terminal.id,
            slot.kind,
            TerminalState::Detached,
            cursor,
            slot.capability,
        )?;
        slot.state = TerminalState::Detached;
        terminal.released = true;
        Ok(())
    }

    /// Records every handle that was dropped without detaching, as `lost`, and
    /// returns what ending each terminal did — including the runs it owned, so
    /// the supervisor stops that work and the session keeps running. A drop is
    /// an owner loss, not evidence about the run.
    pub fn reap(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
    ) -> Result<Vec<TerminalEnd>, Refusal> {
        let mut lost = Vec::new();
        for (id, slot) in self.terminals.iter_mut() {
            if slot.state == TerminalState::Attached && slot.dropped.load(Ordering::Acquire) {
                write_terminal(
                    cx,
                    root,
                    &self.binding.session,
                    id,
                    slot.kind,
                    TerminalState::Lost,
                    None,
                    slot.capability,
                )?;
                slot.state = TerminalState::Lost;
                lost.push((id.clone(), slot.kind));
            }
        }
        // The owned runs are read after the rows are recorded, so a failed read
        // cannot leave a terminal attached to a client that has gone.
        let mut ended = Vec::new();
        for (id, kind) in lost {
            let owned = open_owned_runs(cx, root, &self.binding.session, &id)?;
            ended.push(TerminalEnd {
                terminal: id,
                kind,
                released: false,
                cause: StopCause::OwnerLost,
                owned,
            });
        }
        Ok(ended)
    }

    /// Ends one terminal: EOF without detach, a disconnect, or TERM/HUP.
    ///
    /// The terminal's row is recorded `lost`, and the returned [`TerminalEnd`]
    /// names the runs that terminal admitted and that have not settled — the
    /// only work this call authorises stopping, with the loss as its cause. A
    /// terminal that already detached releases nothing and stops nothing, and
    /// the session run is never in the list: no terminal loss ends the session
    /// (§8.7, §8.11).
    pub fn owner_lost(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        terminal: &TerminalId,
        loss: OwnerLoss,
    ) -> Result<TerminalEnd, Refusal> {
        let Some(slot) = self.terminals.get(terminal) else {
            return Err(Refusal::new(
                RefusalCode::InvalidRequest,
                "this terminal is not attached to the session",
            ));
        };
        let kind = slot.kind;
        if slot.state != TerminalState::Attached {
            // A detached, lost or stopped terminal owns nothing any more.
            return Ok(TerminalEnd {
                terminal: terminal.clone(),
                kind,
                released: true,
                cause: loss.cause(),
                owned: Vec::new(),
            });
        }
        write_terminal(
            cx,
            root,
            &self.binding.session,
            terminal,
            kind,
            TerminalState::Lost,
            None,
            slot.capability,
        )?;
        if let Some(slot) = self.terminals.get_mut(terminal) {
            slot.state = TerminalState::Lost;
        }
        let owned = open_owned_runs(cx, root, &self.binding.session, terminal)?;
        Ok(TerminalEnd {
            terminal: terminal.clone(),
            kind,
            released: false,
            cause: loss.cause(),
            owned,
        })
    }

    /// Tells every attached terminal the container is stopping (§8.7 step 4)
    /// and records that it was told. The session's own settle follows.
    pub fn stopping(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
    ) -> Result<Vec<TerminalId>, Refusal> {
        let mut stopped = Vec::new();
        for (id, slot) in self.terminals.iter_mut() {
            if slot.state == TerminalState::Attached {
                write_terminal(
                    cx,
                    root,
                    &self.binding.session,
                    id,
                    slot.kind,
                    TerminalState::Stopped,
                    None,
                    slot.capability,
                )?;
                slot.state = TerminalState::Stopped;
                stopped.push(id.clone());
            }
        }
        Ok(stopped)
    }

    /// The session's settle ends every grant it issued (§8.3).
    pub fn close(&mut self) {
        self.capability = None;
        self.terminals.clear();
    }

    /// The terminals the authority still holds attached.
    pub fn attached(&self) -> Vec<(TerminalId, TerminalKind)> {
        self.terminals
            .iter()
            .filter(|(_, slot)| slot.state == TerminalState::Attached)
            .map(|(id, slot)| (id.clone(), slot.kind))
            .collect()
    }
}

/// Records that `terminal` admitted `run`, so EOF, TERM/HUP or that terminal's
/// death stops exactly that run. One run has one owning client; recording the
/// same pair twice changes nothing.
pub fn record_owned(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
    terminal: &TerminalId,
    run: RunId,
) -> Result<(), Refusal> {
    if run == session.run() {
        return Err(Refusal::new(
            RefusalCode::InvalidRequest,
            "the session is owned by its environment, not by a terminal",
        ));
    }
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)
        .map_err(|error| unavailable(RunError::from(error)))?;
    store
        .transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            txn.conn().execute(
                "INSERT INTO workspace_session_owned_runs \
                 (run_id, session_run, terminal_id, created_at) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(run_id) DO NOTHING",
                params![
                    run.to_string(),
                    session.run().to_string(),
                    terminal.as_str(),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
        .map_err(unavailable)
}

/// Every run `terminal` admitted, settled or not. A supervisor restart reads
/// this instead of guessing who owned what.
pub fn owned(
    reader: &StoreReader,
    session: &SessionRef,
    terminal: &TerminalId,
) -> Observed<Vec<RunId>> {
    let rows = (|| -> Result<Vec<RunId>, Refusal> {
        let mut statement = reader
            .conn()
            .prepare(&format!(
                "SELECT run_id FROM {OWNED_TABLE} WHERE session_run=?1 AND terminal_id=?2 \
                 ORDER BY created_at, run_id"
            ))
            .map_err(|error| unavailable(RunError::from(error)))?;
        let mapped = statement
            .query_map(
                params![session.run().to_string(), terminal.as_str()],
                |row| row.get::<_, String>(0),
            )
            .map_err(|error| unavailable(RunError::from(error)))?;
        let mut runs = Vec::new();
        for row in mapped {
            let id = row.map_err(|error| unavailable(RunError::from(error)))?;
            runs.push(
                id.parse()
                    .map_err(|_| Refusal::new(RefusalCode::Unknown, "a stored owned run id"))?,
            );
        }
        Ok(runs)
    })();
    match rows {
        Ok(runs) => Observed::Verified(runs, session_evidence()),
        Err(refusal) => {
            Observed::Unknown(UnknownReason::Inconsistent(refusal.diagnostic().to_owned()))
        }
    }
}

/// The runs `terminal` owned that have not settled: the stop list for an owner
/// loss. A settled run is evidence of what happened, not work to stop again.
fn open_owned_runs(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
    terminal: &TerminalId,
) -> Result<Vec<RunId>, Refusal> {
    let observe = cx
        .operation()
        .child(std::time::Duration::from_secs(30))
        .observe()
        .map_err(|_| {
            Refusal::new(
                RefusalCode::Unavailable,
                "the journal could not be observed",
            )
        })?;
    let reader = match Store::read(&observe, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => {
            return Err(Refusal::new(
                RefusalCode::Unknown,
                "the session journal is absent",
            ))
        }
        Observed::Unknown(reason) => {
            return Err(Refusal::new(RefusalCode::Unknown, reason.to_string()))
        }
    };
    let owned = match owned(&reader, session, terminal) {
        Observed::Verified(owned, _) => owned,
        Observed::Absent(_) => Vec::new(),
        Observed::Unknown(reason) => {
            return Err(Refusal::new(RefusalCode::Unknown, reason.to_string()))
        }
    };
    let mut open = Vec::new();
    for run in owned {
        match crate::run::summary(&reader, run) {
            Observed::Verified(summary, _) if summary.phase != Phase::Closed => open.push(run),
            Observed::Verified(_, _) => {}
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => {
                return Err(Refusal::new(RefusalCode::Unknown, reason.to_string()))
            }
        }
    }
    Ok(open)
}

/// Reads the durable terminal rows of one session, newest first.
pub fn terminals(
    reader: &StoreReader,
    session: &SessionRef,
) -> Observed<Vec<(TerminalId, TerminalKind, TerminalState)>> {
    let rows = (|| -> Result<Vec<_>, RunError> {
        let mut statement = reader.conn().prepare(&format!(
            "SELECT terminal_id, kind, state FROM {TABLE} \
             WHERE session_run=?1 ORDER BY created_at DESC, terminal_id"
        ))?;
        let mapped = statement.query_map([session.run().to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut terminals = Vec::new();
        for row in mapped {
            let (id, kind, state) = row?;
            terminals.push((
                TerminalId::parse(&id).map_err(|_| RunError::broken("a stored terminal id"))?,
                TerminalKind::parse(&kind)
                    .map_err(|_| RunError::broken("a stored terminal kind"))?,
                match state.as_str() {
                    "attached" => TerminalState::Attached,
                    "detached" => TerminalState::Detached,
                    "lost" => TerminalState::Lost,
                    "stopped" => TerminalState::Stopped,
                    other => {
                        return Err(RunError::broken(format!(
                            "a stored terminal state `{other}`"
                        )))
                    }
                },
            ));
        }
        Ok(terminals)
    })();
    match rows {
        Ok(rows) => Observed::Verified(rows, session_evidence()),
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_terminal(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
    id: &TerminalId,
    kind: TerminalKind,
    state: TerminalState,
    cursor: Option<StreamCursor>,
    capability: bool,
) -> Result<(), Refusal> {
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)
        .map_err(|error| unavailable(RunError::from(error)))?;
    store
        .transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            // A session that is not current has no terminals: a successor can
            // never inherit a row from a settled run.
            let current = txn
                .conn()
                .query_row(
                    "SELECT generation, phase FROM runs WHERE run_id=?1",
                    [session.run().to_string()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                )
                .optional()
                .map_err(crate::store::StoreError::from)?;
            let current = current.is_some_and(|(generation, phase)| {
                generation as u64 == session.generation().get() && phase != "closed"
            });
            if !current {
                return Err(RunError::refused("the workspace session is not current"));
            }
            txn.conn().execute(
                "INSERT INTO workspace_session_terminals \
                 (terminal_id, session_generation, session_run, environment, \
                  environment_generation, kind, state, cursor, capability, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10) \
                 ON CONFLICT(terminal_id) DO UPDATE SET state=excluded.state, \
                  cursor=coalesce(excluded.cursor, workspace_session_terminals.cursor), \
                  updated_at=excluded.updated_at",
                params![
                    id.as_str(),
                    session.generation().get() as i64,
                    session.run().to_string(),
                    session.environment().name().to_string(),
                    session.environment_generation().get() as i64,
                    kind.as_str(),
                    state.as_str(),
                    cursor.map(|cursor| cursor.sequence() as i64),
                    i64::from(capability),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
        .map_err(unavailable)
}

fn unavailable(error: RunError) -> Refusal {
    Refusal::new(RefusalCode::Unavailable, error.to_string())
}

/// The category a refusal reads as, so a dispatcher can frame it with the
/// kernel's own vocabulary.
pub fn refusal_category(refusal: &Refusal) -> Category {
    match refusal.code() {
        RefusalCode::Unavailable | RefusalCode::Unknown => Category::Unavailable,
        _ => Category::Refused,
    }
}
