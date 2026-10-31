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
    #[serde(default)]
    guest: Option<GuestLocator>,
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
            guest: None,
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
    pub(super) fn require_target(&self, container: &str, boot: &str) -> Result<(), RunError> {
        let guest = self
            .guest
            .as_ref()
            .ok_or(ActivationRefusal::SelectionMismatch)?;
        if guest.target.container_id != container || guest.target.vm_boot_id != boot {
            return Err(ActivationRefusal::TargetEvidenceMismatch.into());
        }
        Ok(())
    }
    pub fn validate(&self) -> Result<(), RunError> {
        if let Some(guest) = &self.guest {
            if self.endpoint != format!("effigy-host:{}", guest.host_bind.port())
                || guest.peer.port() != guest.host_bind.port()
                || guest.host_bind.port() == 0
            {
                return Err(ActivationRefusal::SelectionMismatch.into());
            }
        }
        if self.supervisor_id.trim().is_empty() || self.endpoint.trim().is_empty() {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        if self.pin == Digest([0u8; 32]) {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        Ok(())
    }
    /// Numeric host endpoint for recovery probes, separate from the guest alias.
    pub fn socket(&self) -> Option<SocketAddr> {
        self.guest
            .as_ref()
            .map(|guest| guest.host_bind)
            .or_else(|| self.endpoint.parse().ok())
    }
}

/// Exact container and boot observed by the staged guest transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestLocatorTarget {
    pub container_id: String,
    pub vm_boot_id: String,
}

/// Durable non-secret facts from one guest handshake. The numeric bind remains
/// separate from the alias the guest must dial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GuestLocator {
    target: GuestLocatorTarget,
    peer: SocketAddr,
    host_bind: SocketAddr,
}

/// Launch-issued challenge. Only the callback of `qualify_guest` receives one;
/// its private identity prevents evidence from another launch being replayed.
#[derive(Debug)]
pub struct GuestLocatorChallenge {
    identity: std::sync::Arc<()>,
    session: SessionBinding,
    target: GuestLocatorTarget,
    host_bind: SocketAddr,
    peer: SocketAddr,
    pin: Digest,
}

/// A verified response to a launch-issued challenge. It cannot be deserialized
/// or built from a locator string. Transport remains responsible for TLS proof.
#[derive(Debug)]
pub struct GuestLocatorEvidence {
    challenge: GuestLocatorChallenge,
}

impl GuestLocatorChallenge {
    pub fn host_bind(&self) -> SocketAddr {
        self.host_bind
    }
    pub fn pin(&self) -> &Digest {
        &self.pin
    }
    pub fn endpoint(&self) -> String {
        format!("effigy-host:{}", self.host_bind.port())
    }
    /// Called by the transport only after its exact guest's pinned handshake.
    pub fn verified(
        &self,
        target: &GuestLocatorTarget,
        peer: SocketAddr,
        endpoint: &str,
        pin: &Digest,
    ) -> Result<GuestLocatorEvidence, ServingRefusal> {
        if target != &self.target
            || peer != self.peer
            || endpoint != self.endpoint()
            || pin != &self.pin
        {
            return Err(ServingRefusal::Unqualified);
        }
        Ok(GuestLocatorEvidence {
            challenge: Self {
                identity: self.identity.clone(),
                session: self.session.clone(),
                target: target.clone(),
                host_bind: self.host_bind,
                peer,
                pin: pin.clone(),
            },
        })
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
}

impl SessionOwner {
    pub(crate) fn of(run: RunId, fence: Fence) -> Self {
        Self { run, fence }
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
pub trait ChannelListener: std::fmt::Debug + Send {
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
/// the caller that asked for the session does not drop this, and it releases the
/// listener and wipes the key when the session settles or its owner is
/// abandoned. A session run is adopted by the serving supervisor before
/// [`SessionLaunch::serve`] allows the channel, so a listener is never published
/// as served while nothing owns the run behind it.
#[derive(Debug)]
pub struct SessionLaunch {
    session: SessionRef,
    supervisor_id: String,
    /// The listener and key live here so the session's own settlement can take
    /// and drop them: dropping the listener closes the socket and dropping the
    /// key wipes its bytes.
    state: std::sync::Arc<std::sync::Mutex<LaunchState>>,
}

#[derive(Debug, Default)]
struct LaunchState {
    held: Option<Held>,
    /// Whether the owning run still owns this session (cleared by settlement or
    /// by an explicit handover).
    owned: bool,
    /// Set only by [`SessionLaunch::qualify`] after a verified pinned handshake
    /// through this launch. A listener that arrives already in a serving phase
    /// is still unqualified until then, so no phase flag can mint a locator.
    qualified: bool,
    bound: Option<ChannelBound>,
    guest: Option<GuestLocator>,
    served: bool,
    /// This session run's live-listener claim, released with the launch so a
    /// successor may claim the session again after this one stops.
    claim: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

#[derive(Debug)]
struct Held {
    listener: Option<Box<dyn ChannelListener>>,
    key: Option<ServingKey>,
}

/// Pins owner-drop cleanup (and hence owner-gate release) until publication or
/// completion has committed. No transport I/O runs while this guard is held.
#[derive(Debug)]
pub(crate) struct ServingCustody<'a> {
    state: std::sync::MutexGuard<'a, LaunchState>,
}

impl ServingCustody<'_> {
    pub(crate) fn revalidate(&self, lease: &ServingLease) -> Result<(), ServingRefusal> {
        if self.state.live_bound()?.lease() != lease {
            return Err(ServingRefusal::WrongSession);
        }
        Ok(())
    }
}

impl LaunchState {
    fn endpoint_and_pin(&self) -> Result<(SocketAddr, Digest), ServingRefusal> {
        if !self.owned {
            return Err(ServingRefusal::WrongSession);
        }
        if !self.qualified {
            return Err(ServingRefusal::Unqualified);
        }
        let held = self.held.as_ref().ok_or(ServingRefusal::Stopped)?;
        let listener = held.listener.as_ref().ok_or(ServingRefusal::Stopped)?;
        match listener.phase() {
            ListenerPhase::Stopped => return Err(ServingRefusal::Stopped),
            ListenerPhase::Qualifying => return Err(ServingRefusal::Unqualified),
            ListenerPhase::Holding | ListenerPhase::Serving => {}
        }
        Ok((
            listener.endpoint().ok_or(ServingRefusal::Unbound)?,
            held.key.as_ref().ok_or(ServingRefusal::Stopped)?.pin(),
        ))
    }

    fn live_bound(&self) -> Result<&ChannelBound, ServingRefusal> {
        let (endpoint, pin) = self.endpoint_and_pin()?;
        let bound = self.bound.as_ref().ok_or(ServingRefusal::Unqualified)?;
        if bound.lease.socket() != Some(endpoint) || bound.lease.pin() != &pin {
            return Err(ServingRefusal::Unqualified);
        }
        Ok(bound)
    }
}

impl SessionLaunch {
    /// Take custody of the key and the listener for `session`, on behalf of the
    /// run that owns it. The owner proof comes from that run itself, so custody
    /// is rooted in the detached session owner and never in a binding a caller
    /// assembled. The listener must be bound: an unbound or stopped listener
    /// refuses here, so no launch can later publish an endpoint that is not
    /// listening. The launch registers itself with the run: settlement of the
    /// session closes the listener and wipes the key.
    pub fn launch(
        session_run: &mut Run<'_>,
        session: &SessionRef,
        supervisor_id: impl Into<String>,
        key: ServingKey,
        listener: Box<dyn ChannelListener>,
    ) -> Result<Self, ServingRefusal> {
        let owner = session_run
            .session_owner()
            .map_err(|_| ServingRefusal::WrongSession)?;
        if !owner.names(session) {
            return Err(ServingRefusal::WrongSession);
        }
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
        let state = std::sync::Arc::new(std::sync::Mutex::new(LaunchState {
            held: Some(Held {
                listener: Some(listener),
                key: Some(key),
            }),
            owned: true,
            qualified: false,
            bound: None,
            guest: None,
            served: false,
            claim: None,
        }));
        // The session run is the custody: settling it takes the listener and the
        // key away, so a settled session neither serves nor keeps a socket open.
        let settled = state.clone();
        session_run.on_settle(move || release_state(&settled));
        Ok(Self {
            session: session.clone(),
            supervisor_id,
            state,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, LaunchState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        self.lock()
            .held
            .as_ref()
            .and_then(|held| held.listener.as_ref())
            .map(|listener| listener.phase())
            .unwrap_or(ListenerPhase::Stopped)
    }
    /// The endpoint the listener actually bound, while it is still bound.
    pub fn endpoint(&self) -> Option<SocketAddr> {
        self.lock()
            .held
            .as_ref()
            .and_then(|held| held.listener.as_ref())
            .and_then(|listener| listener.endpoint())
    }

    /// Qualify the endpoint the launch holds: a bounded pinned handshake,
    /// against the key this launch still holds, at the address it actually
    /// bound. Only a verified answer sets the launch's qualified flag; an absent
    /// or unknown one refuses, so a listener is never published on a promise.
    pub fn qualify(
        &mut self,
        probe: &mut dyn ServingProbe,
        deadline: Duration,
    ) -> Result<(), ServingRefusal> {
        let endpoint = {
            let state = self.lock();
            if !state.owned {
                return Err(ServingRefusal::WrongSession);
            }
            let held = state.held.as_ref().ok_or(ServingRefusal::Stopped)?;
            let listener = held.listener.as_ref().ok_or(ServingRefusal::Stopped)?;
            if listener.phase() == ListenerPhase::Stopped {
                return Err(ServingRefusal::Stopped);
            }
            listener.endpoint().ok_or(ServingRefusal::Unbound)?
        };
        let pin = {
            let state = self.lock();
            state
                .held
                .as_ref()
                .and_then(|held| held.key.as_ref())
                .map(ServingKey::pin)
                .ok_or(ServingRefusal::Stopped)?
        };
        match probe.pinned_handshake(endpoint, &pin, deadline) {
            Observed::Verified(..) => {
                let mut state = self.lock();
                let Some(held) = state.held.as_mut() else {
                    return Err(ServingRefusal::Stopped);
                };
                let Some(listener) = held.listener.as_mut() else {
                    return Err(ServingRefusal::Stopped);
                };
                listener.qualify()?;
                state.qualified = true;
                Ok(())
            }
            Observed::Absent(_) | Observed::Unknown(_) => Err(ServingRefusal::Unqualified),
        }
    }

    /// Qualify the guest alias against this live launch. No lock spans guest
    /// I/O; custody and endpoint/key identity are rechecked before retention.
    pub fn qualify_guest(
        &mut self,
        session_run: &Run<'_>,
        target: GuestLocatorTarget,
        peer: std::net::IpAddr,
        handshake: impl FnOnce(&GuestLocatorChallenge) -> Result<GuestLocatorEvidence, ServingRefusal>,
    ) -> Result<(), ServingRefusal> {
        self.require_live_owner(session_run)?;
        let (endpoint, pin) = {
            let state = self.lock();
            let held = state.held.as_ref().ok_or(ServingRefusal::Stopped)?;
            let listener = held.listener.as_ref().ok_or(ServingRefusal::Stopped)?;
            if listener.phase() == ListenerPhase::Stopped {
                return Err(ServingRefusal::Stopped);
            }
            (
                listener.endpoint().ok_or(ServingRefusal::Unbound)?,
                held.key.as_ref().ok_or(ServingRefusal::Stopped)?.pin(),
            )
        };
        if peer.is_unspecified()
            || peer.is_multicast()
            || endpoint.port() == 0
            || target.container_id.trim().is_empty()
            || target.vm_boot_id.trim().is_empty()
        {
            return Err(ServingRefusal::Unqualified);
        }
        let challenge = GuestLocatorChallenge {
            identity: std::sync::Arc::new(()),
            session: self.binding(),
            target,
            host_bind: endpoint,
            peer: SocketAddr::new(peer, endpoint.port()),
            pin: pin.clone(),
        };
        let answer = handshake(&challenge)?.challenge;
        if !std::sync::Arc::ptr_eq(&challenge.identity, &answer.identity)
            || answer.session != self.binding()
        {
            return Err(ServingRefusal::WrongSession);
        }
        self.require_live_owner(session_run)?;
        let mut state = self.lock();
        let guest = GuestLocator {
            target: answer.target,
            peer: answer.peer,
            host_bind: endpoint,
        };
        if state.bound.is_some() && state.guest.as_ref() != Some(&guest) {
            return Err(ServingRefusal::AlreadyPublished);
        }
        let held = state.held.as_mut().ok_or(ServingRefusal::Stopped)?;
        let listener = held.listener.as_mut().ok_or(ServingRefusal::Stopped)?;
        if listener.endpoint() != Some(endpoint)
            || held.key.as_ref().map(ServingKey::pin) != Some(pin)
        {
            return Err(ServingRefusal::Unqualified);
        }
        listener.qualify()?;
        state.qualified = true;
        state.guest = Some(guest);
        Ok(())
    }

    /// Stop the listener the launch holds. Idempotent; the lease is dropped
    /// with it, so a stopped launch reports no locator, while the launch still
    /// belongs to its run until it is released or the run settles.
    pub fn stop(&mut self) {
        let mut state = self.lock();
        state.bound = None;
        state.served = false;
        if let Some(mut held) = state.held.take() {
            if let Some(mut listener) = held.listener.take() {
                listener.stop();
            }
            held.key.take();
        }
    }

    /// Publish the locator this generation serves. It requires a bound
    /// listener, a qualification answer, the key still in custody and the
    /// session run still live; a retry answers the same [`ChannelBound`].
    pub fn publish(&mut self, session_run: &Run<'_>) -> Result<ChannelBound, ServingRefusal> {
        self.require_live_owner(session_run)?;
        let mut state = self.lock();
        let (endpoint, pin) = state.endpoint_and_pin()?;
        if state.bound.is_some() {
            return Ok(state.live_bound()?.clone());
        }
        let binding = self.binding();
        let mut lease =
            ServingLease::published(binding.clone(), self.supervisor_id.clone(), endpoint, pin);
        if let Some(guest) = &state.guest {
            lease.endpoint = format!("effigy-host:{}", endpoint.port());
            lease.guest = Some(guest.clone());
        }
        if lease.validate().is_err() {
            return Err(ServingRefusal::Unqualified);
        }
        let bound = ChannelBound {
            session: binding,
            lease,
        };
        state.bound = Some(bound.clone());
        Ok(bound)
    }

    /// The published lease, if this launch published one.
    pub fn lease(&self) -> Option<ServingLease> {
        self.lock()
            .live_bound()
            .ok()
            .map(|bound| bound.lease().clone())
    }

    pub(crate) fn hold_lease(
        &self,
        lease: &ServingLease,
    ) -> Result<ServingCustody<'_>, ServingRefusal> {
        let state = self.lock();
        if state.live_bound()?.lease() != lease || self.binding() != *lease.session() {
            return Err(ServingRefusal::WrongSession);
        }
        Ok(ServingCustody { state })
    }

    /// The serving supervisor adopted the session run: only now may the channel
    /// serve. A lease that was never published cannot serve, and neither can one
    /// whose session run is no longer open.
    pub fn serve(&mut self, session_run: &Run<'_>) -> Result<(), ServingRefusal> {
        self.require_live_owner(session_run)?;
        {
            // One session is served through one owner: a second launch that asks
            // to serve the same session refuses here instead of dispatching on a
            // second listener and key under one session's custody.
            let state = self.lock();
            if state.claim.is_none() {
                drop(state);
                return self.claim_and_serve(session_run);
            }
        }
        let mut state = self.lock();
        if state.bound.is_none() {
            return Err(ServingRefusal::Unqualified);
        }
        state.live_bound()?;
        state.served = true;
        Ok(())
    }

    /// Claim the session's one serving owner and mark this launch serving.
    fn claim_and_serve(&mut self, session_run: &Run<'_>) -> Result<(), ServingRefusal> {
        let claim = session_run.claim_session_launch()?;
        let mut state = self.lock();
        if state.bound.is_none() {
            claim.store(false, std::sync::atomic::Ordering::SeqCst);
            return Err(ServingRefusal::Unqualified);
        }
        state.live_bound()?;
        state.claim = Some(claim);
        state.served = true;
        Ok(())
    }

    /// Whether the v1 channel may dispatch on this launch's listener.
    pub fn serving(&self, session_run: &Run<'_>) -> bool {
        let state = self.lock();
        state.served && self.session_is_live(session_run) && state.live_bound().is_ok()
    }

    /// Release the owning run's custody while the launch lives: the session is
    /// no longer owned here, so nothing this launch published may be served.
    pub fn abandon_owner(&mut self) {
        release_state(&self.state);
    }

    /// Whether this launch still holds the session's owner custody.
    pub fn owned(&self) -> bool {
        self.lock().owned
    }

    /// Whether the session's key is still in custody (it is dropped, and wiped,
    /// the moment the session settles or the launch releases it).
    pub fn key_held(&self) -> bool {
        self.lock()
            .held
            .as_ref()
            .map(|held| held.key.is_some())
            .unwrap_or(false)
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

    fn require_live_owner(&self, session_run: &Run<'_>) -> Result<(), ServingRefusal> {
        if !self.lock().owned {
            return Err(ServingRefusal::WrongSession);
        }
        if self.session_is_live(session_run) {
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
        if !self.lock().owned || !self.session_is_live(session_run) {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            });
        }
        let state = self.lock();
        let Ok(bound) = state.live_bound() else {
            return Observed::Absent(AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            });
        };
        let Some(endpoint) = bound.lease.socket() else {
            return Observed::Unknown(UnknownReason::Unparsable(
                "the published endpoint is not a socket address".into(),
            ));
        };
        probe.pinned_handshake(endpoint, &bound.lease.pin, deadline)
    }

    /// Release the session's custody: stop the listener and wipe the key. This
    /// is the only path that does, so a superseded session keeps its listener
    /// until its own settlement.
    pub fn release(self) {
        release_state(&self.state);
    }
}

impl Drop for SessionLaunch {
    fn drop(&mut self) {
        // A launch that is dropped without an explicit stop still gives the
        // session back: its listener, key and one-owner claim are released, so a
        // superseded or failed preparation never blocks the session's next one.
        release_state(&self.state);
    }
}

/// Drop the listener and the key, closing the socket and wiping the secret.
fn release_state(state: &std::sync::Arc<std::sync::Mutex<LaunchState>>) {
    let mut state = match state.lock() {
        Ok(state) => state,
        Err(poisoned) => poisoned.into_inner(),
    };
    state.owned = false;
    state.bound = None;
    state.served = false;
    if let Some(claim) = state.claim.take() {
        claim.store(false, std::sync::atomic::Ordering::SeqCst);
    }
    if let Some(mut held) = state.held.take() {
        if let Some(mut listener) = held.listener.take() {
            listener.stop();
        }
        held.key.take();
    }
}
