//! The session-owned channel listener and its key (052 §8.11).
//!
//! One workspace session owns one listener and one memory-only serving key for
//! the durable lifetime of that session, not the lifetime of the command that
//! asked for it. This module owns that custody as a typed value:
//! [`SessionLaunch`] holds the session binding, the key and the bound listener,
//! and [`ChannelBound`] is the *only* value that names a locator, so a listener
//! is never advertised as served before it is actually bound. The lease a
//! selection commits is [`ServingLease`], the same endpoint and pin
//! [`ChannelBound`] reports, so a published generation, its obligation and the
//! listener a guest reaches are one fact.
//!
//! The kernel binds a socket and owns its lifetime; it does no TLS and
//! interprets no application frame. Transport, pin verification and the
//! handshake that proves key possession belong to #483, injected here as
//! [`ServingProbe`] (and, for the transport itself, behind [`ChannelListener`]).
//! A dead listener is never recreated under an old locator: the next activation
//! uses a new session, generation, endpoint and pin, and superseded sessions
//! keep their own listener and holders until their own settlement.

use super::*;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

/// Where a listener is in its life. Only `Holding` and `Serving` have answered a
/// qualification, and only `Serving` has an owner that holds the session run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenerPhase {
    /// Bound, but nothing has qualified it yet.
    Qualifying,
    /// Qualified: one bounded pinned liveness probe at a time, no application
    /// frame, no credential, no admission.
    Holding,
    /// The owning session run was adopted; the v1 channel may serve.
    Serving,
    /// Stopped. A stopped listener can never yield [`ChannelBound`].
    Stopped,
}

impl ListenerPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qualifying => "qualifying",
            Self::Holding => "holding",
            Self::Serving => "serving",
            Self::Stopped => "stopped",
        }
    }
    /// Whether a connection may be accepted at all. A stopped, unbound or
    /// still-qualifying listener is not a server.
    pub fn accepts(self) -> bool {
        matches!(self, Self::Holding | Self::Serving)
    }
}

/// Why a session launch could not hold, publish or serve its listener.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServingRefusal {
    /// The listener is not bound, so there is no endpoint to publish.
    Unbound,
    /// The listener was stopped; its locator may not be reused.
    Stopped,
    /// The listener has not answered a qualification.
    Unqualified,
    /// Only an adopted session run serves the channel.
    NotAdopted,
    /// The session already published a lease; one session has one listener.
    AlreadyPublished,
    /// The launch names another session than the binding it was given.
    WrongSession,
}

impl ServingRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unbound => "unbound",
            Self::Stopped => "stopped",
            Self::Unqualified => "unqualified",
            Self::NotAdopted => "not_adopted",
            Self::AlreadyPublished => "already_published",
            Self::WrongSession => "wrong_session",
        }
    }
}

impl std::fmt::Display for ServingRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unbound => "the session listener is not bound, so nothing may be published",
            Self::Stopped => "the session listener is stopped; its locator may not be reused",
            Self::Unqualified => "the session listener has not been qualified",
            Self::NotAdopted => "only an adopted session run serves the channel",
            Self::AlreadyPublished => "the session already published its listener",
            Self::WrongSession => "the launch names another session than the binding it was given",
        })
    }
}

impl From<ServingRefusal> for RunError {
    fn from(refusal: ServingRefusal) -> Self {
        Self::Refused(refusal.to_string())
    }
}

/// The endpoint and pin one published generation's listener serves. It is what
/// a selection records, so a client's locator and the listener the guest
/// reaches are one fact, and it names the exact session that holds the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServingLease {
    /// The session whose launch holds the key and the bound listener. A lease
    /// for another session is never a valid selection for this one.
    session: SessionBinding,
    supervisor_id: String,
    endpoint: String,
    pin: Digest,
}

impl ServingLease {
    /// Only [`SessionLaunch::publish`] builds one, from a listener that is
    /// actually bound and qualified and the key that launch still holds. A
    /// durable receipt reads one back as data; a new one is never fabricated.
    fn published(
        session: SessionBinding,
        supervisor_id: String,
        endpoint: SocketAddr,
        pin: Digest,
    ) -> Self {
        Self {
            session,
            supervisor_id,
            endpoint: endpoint.to_string(),
            pin,
        }
    }
    /// The session whose launch must hold this listener and key.
    pub fn session(&self) -> &SessionBinding {
        &self.session
    }
    pub fn supervisor_id(&self) -> &str {
        &self.supervisor_id
    }
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    pub fn pin(&self) -> &Digest {
        &self.pin
    }
    pub fn validate(&self) -> Result<(), RunError> {
        if self.supervisor_id.trim().is_empty() || self.endpoint.trim().is_empty() {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        if self.pin == Digest([0u8; 32]) {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        Ok(())
    }
    /// Address the probe dials. A locator that is not a socket address is a
    /// locator nothing can reach, so it validates but never probes.
    pub fn socket(&self) -> Option<SocketAddr> {
        self.endpoint.parse().ok()
    }
}

/// The proof that the caller holds the admitted workspace session's owning run.
///
/// Only [`Run::session_owner`](crate::run::Run::session_owner) builds one, from
/// the run whose private session slot is the journal's record of that session.
/// A listener and key can therefore only be launched for a session whose run
/// the caller actually owns; a copied binding, a made-up run or an environment
/// run cannot hold custody.
#[derive(Debug)]
pub struct SessionOwner {
    run: RunId,
    fence: Fence,
    /// Whether this proof still owns the session. The session's *gate* stays
    /// with its run, so settlement releases it; this flag is the explicit
    /// handover a launch can drop when the owner goes away.
    held: bool,
}

impl SessionOwner {
    pub(crate) fn of(run: RunId, fence: Fence) -> Self {
        Self {
            run,
            fence,
            held: true,
        }
    }
    /// Whether this proof still holds the session's owner custody.
    pub fn held(&self) -> bool {
        self.held
    }
    /// Release the owner proof early: the session is no longer owned here, so
    /// nothing it published may keep serving.
    fn abandon(&mut self) {
        self.held = false;
    }
    pub fn run(&self) -> RunId {
        self.run
    }
    pub fn fence(&self) -> &Fence {
        &self.fence
    }
    /// Whether this proof is the owning run of `session`, with its own fence.
    fn names(&self, session: &SessionRef) -> bool {
        self.run == session.run() && self.fence == *session.fence()
    }
}

/// The session's memory-only serving key.
///
/// The kernel neither reads nor encodes the secret bytes: it owns their
/// lifetime and the public pin the transport derived from them, and it wipes
/// them when the launch that held them is released. Transport generation
/// belongs to #483, which hands the bytes here.
pub struct ServingKey {
    secret: Vec<u8>,
    pin: Digest,
}

impl ServingKey {
    /// Wrap the transport's generated key material. A zero-length secret or a
    /// zero pin is refused: both are what an uninitialized key looks like.
    pub fn new(secret: Vec<u8>, pin: Digest) -> Result<Self, ServingRefusal> {
        if secret.is_empty() || pin == Digest([0u8; 32]) {
            return Err(ServingRefusal::Unbound);
        }
        Ok(Self { secret, pin })
    }
    pub fn pin(&self) -> Digest {
        self.pin.clone()
    }
    /// The transport borrows the bytes to build its server configuration. The
    /// kernel never copies them into a record, a log or a diagnostic.
    pub fn borrow_secret(&self) -> &[u8] {
        &self.secret
    }
}

impl std::fmt::Debug for ServingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never the secret, not even its length.
        f.write_str("ServingKey(<redacted>)")
    }
}

impl Drop for ServingKey {
    fn drop(&mut self) {
        self.secret.fill(0);
    }
}

/// One bound listener, as the session owner holds it. The kernel's own
/// implementation binds a loopback socket and accepts nothing itself; a
/// transport implementation (#483) or a fake in a test supplies the same three
/// facts: its endpoint, its phase and an idempotent stop.
pub trait ChannelListener: std::fmt::Debug {
    fn endpoint(&self) -> Option<SocketAddr>;
    fn phase(&self) -> ListenerPhase;
    /// Record that the transport qualified this endpoint: the session's pin
    /// answered a handshake here. A stopped listener cannot qualify.
    fn qualify(&mut self) -> Result<(), ServingRefusal>;
    /// Close the socket. Idempotent: a second stop is not a failure.
    fn stop(&mut self);
}

/// A real bound socket. Binding happens in [`HostChannelListener::bind`], so an
/// unbound listener does not exist as a value at all and cannot be published.
#[derive(Debug)]
pub struct HostChannelListener {
    listener: Option<TcpListener>,
    endpoint: SocketAddr,
    phase: ListenerPhase,
}

impl HostChannelListener {
    /// Bind the exact address the selection will publish. Port 0 asks the OS for
    /// one; the endpoint comes back from the bound socket, never from a guess.
    pub fn bind(address: SocketAddr) -> std::io::Result<Self> {
        let listener = TcpListener::bind(address)?;
        let endpoint = listener.local_addr()?;
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener: Some(listener),
            endpoint,
            phase: ListenerPhase::Qualifying,
        })
    }
    /// Qualify: the transport proved the pin answered on this endpoint.
    pub fn qualify(&mut self) -> Result<(), ServingRefusal> {
        match self.phase {
            ListenerPhase::Qualifying => {
                self.phase = ListenerPhase::Holding;
                Ok(())
            }
            ListenerPhase::Holding => Ok(()),
            ListenerPhase::Serving => Ok(()),
            ListenerPhase::Stopped => Err(ServingRefusal::Stopped),
        }
    }
    /// The `Holding` phase probe: accept at most one connection and close it
    /// having written nothing. It carries no schema, no version, no `hello`, no
    /// credential and no admission record.
    pub fn accept_probe(&self) -> std::io::Result<Option<TcpStream>> {
        if !self.phase.accepts() {
            return Ok(None);
        }
        match &self.listener {
            Some(listener) => match listener.accept() {
                Ok((stream, _)) => Ok(Some(stream)),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                Err(error) => Err(error),
            },
            None => Ok(None),
        }
    }
}

impl ChannelListener for HostChannelListener {
    fn endpoint(&self) -> Option<SocketAddr> {
        Some(self.endpoint)
    }
    fn phase(&self) -> ListenerPhase {
        self.phase
    }
    fn qualify(&mut self) -> Result<(), ServingRefusal> {
        HostChannelListener::qualify(self)
    }
    fn stop(&mut self) {
        self.listener = None;
        self.phase = ListenerPhase::Stopped;
    }
}

/// A bounded pinned handshake against a lease, supplied by the transport. The
/// kernel asks; it does not implement TLS. `Verified` means the process holding
/// the key that hashes to the lease's pin answered and accepted; `Absent` and
/// `Unknown` are both "no serving liveness" and neither authorises anything.
pub trait ServingProbe {
    fn pinned_handshake(
        &mut self,
        endpoint: SocketAddr,
        pin: &Digest,
        deadline: Duration,
    ) -> Observed<()>;
}

/// A bounded verification of the exact container and VM boot a publication
/// names, supplied by the owning engine. The kernel does not inspect a backend
/// itself; recovery asks, and only a verified answer lets a generation complete.
pub trait TargetProbe {
    fn verify_target(
        &mut self,
        container_id: &str,
        vm_boot_id: &str,
        deadline: Duration,
    ) -> Observed<()>;
}

/// The published locator. Only [`SessionLaunch::publish`] constructs one, and
/// only from a bound listener that answered qualification and whose key is
/// still held, so "published" cannot mean "maybe bound".
#[derive(Debug, Clone)]
pub struct ChannelBound {
    session: SessionBinding,
    lease: ServingLease,
}

impl ChannelBound {
    pub fn session(&self) -> &SessionBinding {
        &self.session
    }
    pub fn lease(&self) -> &ServingLease {
        &self.lease
    }
}

/// One detached session's listener and key custody.
///
/// The launch is what a supervisor holds for the session's whole life: dropping
/// the caller that asked for the session does not drop this, and
/// [`SessionLaunch::release`] is the only thing that stops the listener and
/// wipes the key. A session run is adopted by the serving supervisor before
/// [`SessionLaunch::serve`] allows the channel, so a listener is never published
/// as served while nothing owns the run behind it.
#[derive(Debug)]
pub struct SessionLaunch {
    session: SessionRef,
    owner: SessionOwner,
    supervisor_id: String,
    key: ServingKey,
    listener: Box<dyn ChannelListener>,
    /// Set only by [`SessionLaunch::qualify`] after a verified pinned handshake
    /// through this launch. A listener that arrives already in a serving phase
    /// is still unqualified until then, so no phase flag can mint a locator.
    qualified: bool,
    bound: Option<ChannelBound>,
    served: bool,
}

impl SessionLaunch {
    /// Take custody of the key and the listener for `session`, on behalf of the
    /// run that owns it. `owner` is only obtainable from that session's own
    /// `Run`, so custody is always rooted in the detached session owner and never
    /// in a binding a caller assembled. The listener must be bound: an unbound or
    /// stopped listener refuses here, so no launch can later publish an endpoint
    /// that is not listening.
    pub fn launch(
        session: &SessionRef,
        owner: SessionOwner,
        supervisor_id: impl Into<String>,
        key: ServingKey,
        listener: Box<dyn ChannelListener>,
    ) -> Result<Self, ServingRefusal> {
        if !owner.names(session) {
            return Err(ServingRefusal::WrongSession);
        }
        let session = session.clone();
        let supervisor_id = supervisor_id.into();
        if supervisor_id.trim().is_empty() {
            return Err(ServingRefusal::Unqualified);
        }
        match listener.phase() {
            ListenerPhase::Stopped => return Err(ServingRefusal::Stopped),
            ListenerPhase::Qualifying | ListenerPhase::Holding | ListenerPhase::Serving => {}
        }
        if listener.endpoint().is_none() {
            return Err(ServingRefusal::Unbound);
        }
        Ok(Self {
            session,
            owner,
            supervisor_id,
            key,
            listener,
            qualified: false,
            bound: None,
            served: false,
        })
    }

    pub fn session(&self) -> &SessionRef {
        &self.session
    }
    /// The lease-facing identity of the session this launch holds custody for.
    pub fn binding(&self) -> SessionBinding {
        SessionBinding {
            owner: self.session.fence().resource.clone(),
            run: self.session.run(),
            generation: self.session.generation(),
        }
    }
    pub fn phase(&self) -> ListenerPhase {
        self.listener.phase()
    }
    /// Qualify the endpoint the launch holds: a bounded pinned handshake,
    /// against the key this launch still holds, at the address it actually
    /// bound. Only a verified answer moves the listener to `Holding`; an absent
    /// or unknown one refuses, so a listener is never published on a promise.
    pub fn qualify(
        &mut self,
        probe: &mut dyn ServingProbe,
        deadline: Duration,
    ) -> Result<(), ServingRefusal> {
        if self.listener.phase() == ListenerPhase::Stopped {
            return Err(ServingRefusal::Stopped);
        }
        let endpoint = self.listener.endpoint().ok_or(ServingRefusal::Unbound)?;
        match probe.pinned_handshake(endpoint, &self.key.pin(), deadline) {
            Observed::Verified(..) => {
                self.listener.qualify()?;
                self.qualified = true;
                Ok(())
            }
            Observed::Absent(_) | Observed::Unknown(_) => Err(ServingRefusal::Unqualified),
        }
    }
    /// Stop the listener the launch holds. Idempotent; the lease is dropped
    /// with it, so a stopped launch reports no locator.
    pub fn stop(&mut self) {
        self.listener.stop();
        self.bound = None;
        self.served = false;
    }
    /// The endpoint the listener actually bound, while it is still bound.
    pub fn endpoint(&self) -> Option<SocketAddr> {
        self.listener.endpoint()
    }

    /// Publish the locator this generation serves. It requires a bound
    /// listener, a qualification answer and the key still in custody, and it
    /// answers the same [`ChannelBound`] to a retry.
    pub fn publish(&mut self) -> Result<&ChannelBound, ServingRefusal> {
        self.require_custody()?;
        if !self.qualified {
            return Err(ServingRefusal::Unqualified);
        }
        if self.bound.is_none() {
            return self.publish_once();
        }
        self.bound.as_ref().ok_or(ServingRefusal::Unbound)
    }

    /// Publish a first locator. The lease is built from the endpoint the
    /// listener actually bound and the key still in custody, so "published"
    /// cannot mean "maybe bound".
    fn publish_once(&mut self) -> Result<&ChannelBound, ServingRefusal> {
        match self.listener.phase() {
            ListenerPhase::Stopped => return Err(ServingRefusal::Stopped),
            ListenerPhase::Qualifying => return Err(ServingRefusal::Unqualified),
            ListenerPhase::Holding | ListenerPhase::Serving => {}
        }
        let endpoint = self.listener.endpoint().ok_or(ServingRefusal::Unbound)?;
        let binding = self.binding();
        let bound = ChannelBound {
            session: binding.clone(),
            lease: ServingLease::published(
                binding,
                self.supervisor_id.clone(),
                endpoint,
                self.key.pin(),
            ),
        };
        // The lease is built from a bound endpoint and a held key, so this is
        // a shape check on values that already passed the refusals above.
        if bound.lease.validate().is_err() {
            return Err(ServingRefusal::Unqualified);
        }
        self.bound = Some(bound);
        self.bound.as_ref().ok_or(ServingRefusal::Unbound)
    }

    /// The published lease, if this launch published one.
    pub fn lease(&self) -> Option<&ServingLease> {
        self.bound.as_ref().map(ChannelBound::lease)
    }

    /// The serving supervisor adopted the session run: only now may the channel
    /// serve. A lease that was never published cannot serve, and neither can one
    /// whose owner custody this launch no longer holds.
    pub fn serve(&mut self, session_run: &Run<'_>) -> Result<(), ServingRefusal> {
        self.require_custody()?;
        self.require_live_session(session_run)?;
        if !self.qualified || self.bound.is_none() {
            return Err(ServingRefusal::Unqualified);
        }
        match self.listener.phase() {
            ListenerPhase::Stopped => Err(ServingRefusal::Stopped),
            ListenerPhase::Qualifying => Err(ServingRefusal::Unqualified),
            ListenerPhase::Holding | ListenerPhase::Serving => {
                self.served = true;
                Ok(())
            }
        }
    }

    /// Whether the v1 channel may dispatch on this launch's listener.
    pub fn serving(&self, session_run: &Run<'_>) -> bool {
        self.served
            && self.qualified
            && self.owner.held()
            && self.session_is_live(session_run)
            && self.listener.phase().accepts()
    }

    /// A settled or replaced session run serves nothing, whatever this launch
    /// once published: the locator is only callable while the run it names is
    /// still open at its own fence.
    fn session_is_live(&self, session_run: &Run<'_>) -> bool {
        session_run.id() == self.session.run()
            && session_run.fence() == self.session.fence()
            && !matches!(
                session_run.phase(),
                Ok(crate::run::Phase::Stopping) | Ok(crate::run::Phase::Closed) | Err(_)
            )
    }

    fn require_live_session(&self, session_run: &Run<'_>) -> Result<(), ServingRefusal> {
        if self.session_is_live(session_run) {
            Ok(())
        } else {
            Err(ServingRefusal::WrongSession)
        }
    }

    /// Release the owning run's custody while the launch lives: the session is
    /// no longer owned here, so nothing this launch published may be served.
    pub fn abandon_owner(&mut self) {
        self.owner.abandon();
        self.bound = None;
        self.served = false;
    }

    /// Whether this launch still holds the session's owner custody.
    pub fn owned(&self) -> bool {
        self.owner.held()
    }

    fn require_custody(&self) -> Result<(), ServingRefusal> {
        if self.owner.held() {
            Ok(())
        } else {
            Err(ServingRefusal::WrongSession)
        }
    }

    /// One bounded pinned liveness probe, for recovery to judge a selection
    /// before it completes. It proves the key holder is alive and accepting; it
    /// grants nothing and, once the launch serves, is no longer needed.
    pub fn liveness(
        &self,
        session_run: &Run<'_>,
        probe: &mut dyn ServingProbe,
        deadline: Duration,
    ) -> Observed<()> {
        if !self.session_is_live(session_run) {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            });
        }
        let Some(bound) = &self.bound else {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            });
        };
        let Some(endpoint) = bound.lease.socket() else {
            return Observed::Unknown(UnknownReason::Unparsable(
                "the published endpoint is not a socket address".into(),
            ));
        };
        if !self.listener.phase().accepts() {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            });
        }
        probe.pinned_handshake(endpoint, &bound.lease.pin, deadline)
    }

    /// Release the session's custody: stop the listener and wipe the key. This
    /// is the only path that does, so a superseded session keeps its listener
    /// until its own settlement and a dead one can never be rebound under the
    /// same locator.
    pub fn release(mut self) {
        self.bound = None;
        self.served = false;
        self.listener.stop();
        // `self.key` drops here and fills its bytes with zero.
    }
}
