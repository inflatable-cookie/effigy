//! The host's detached serving owner for one prepared workspace channel
//! (contract 052 §8.7 and §8.11, the E10 handoff).
//!
//! A prepared channel is a session's listener and key held by
//! [`SessionLaunch`], plus the [`HostChannelAcceptor`] that receives TLS
//! connections. Nothing here creates either: the launch exists only for a run
//! that already carries the session's custody (`SessionSupervisor::adopt`), and
//! it can only publish a locator after a pinned handshake qualified its
//! endpoint. This module is the consumer those facts were waiting for: it marks
//! the launch serving *before* it accepts anything, then feeds each accepted
//! connection to the kernel's channel dispatcher.
//!
//! It owns no admission, credential, resolution or effect: the dispatcher is
//! the caller's, with its owning engines' seams, and every request it answers
//! passes the kernel's own pin check, session fence and grants.

use std::io;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use effigy_kernel::ctx::{EffectContext, ObserveContext};
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Run, RunError};
use effigy_kernel::workspace_channel::{self as channel, Refusal, Served, Streams, Transport};
use effigy_kernel::workspace_session::SessionRef;
use effigy_kernel::workspace_session::{ServingRefusal, SessionLaunch, SessionSupervisor};

use crate::runner::workspace_transport::listener::{HostChannelAcceptor, ServerTlsTransport};
use crate::runner::workspace_transport::qualification::{
    PreparedHostChannel, QualifiedHostLocator,
};

/// Where accepted connections come from.
///
/// Production accepts through the session's bound TLS listener
/// ([`HostChannelAcceptor`]); a proof supplies scripted transports with the same
/// `Transport` contract. Accepting is bounded and never blocks the owner.
pub(crate) trait ChannelAccept {
    type Transport: Transport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<Self::Transport>>;
}

impl ChannelAccept for HostChannelAcceptor {
    type Transport = ServerTlsTransport;
    fn accept(
        &mut self,
        launch: &SessionLaunch,
        run: &Run<'_>,
    ) -> io::Result<Option<ServerTlsTransport>> {
        HostChannelAcceptor::accept(self, launch, run)
    }
}

/// Why the serving owner did not take or finish a connection.
#[derive(Debug)]
pub(crate) enum ServeError {
    /// The session's own custody refused — unqualified, not adopted, stopped or
    /// another session's — so nothing was accepted.
    Refused(ServingRefusal),
    /// The transport could not accept or complete the connection.
    Transport(io::Error),
    /// The channel refused this connection (pin, negotiation, credential or a
    /// request). That connection is over; the session keeps serving.
    Channel(Refusal),
    /// This owner no longer holds the launch.
    Stopped,
}

impl ServeError {
    /// Whether the owner can serve another connection after this failure. A
    /// refused custody or a released launch is terminal; a refused connection
    /// is not.
    pub(crate) fn is_terminal(&self) -> bool {
        matches!(self, Self::Refused(_) | Self::Stopped)
    }
}

impl From<ServingRefusal> for ServeError {
    fn from(refusal: ServingRefusal) -> Self {
        Self::Refused(refusal)
    }
}
impl From<io::Error> for ServeError {
    fn from(error: io::Error) -> Self {
        Self::Transport(error)
    }
}
impl From<Refusal> for ServeError {
    fn from(refusal: Refusal) -> Self {
        Self::Channel(refusal)
    }
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(refusal) => {
                write!(formatter, "{}", RunError::from(*refusal))
            }
            Self::Transport(error) => write!(formatter, "workspace channel accept failed: {error}"),
            Self::Channel(refusal) => write!(formatter, "workspace channel refused: {refusal}"),
            Self::Stopped => formatter.write_str("the workspace serving owner was released"),
        }
    }
}

/// The frozen activation's owned channel, exactly as the environment hands it
/// to this module: the session run it owns, the session identity, and the
/// prepared listener/key/locator. Nothing here rebuilds any of them.
///
/// The activation produces this after its environment commit, its CAS receipt
/// and its `selection_switched` record, and must not keep, drop or recreate the
/// parts when its own call returns: ownership moves here, and the serving owner
/// retains them for the session's whole life.
pub(crate) struct OwnedWorkspaceChannel<'a> {
    /// The frozen session's owning run, already adopted under its supervisor
    /// id. A `PreparedHostChannel` cannot exist before that adoption: the
    /// kernel's `SessionLaunch::launch` requires the run's custody record
    /// (`Run::session_owner`), so the activation adopts first and the channel is
    /// prepared against the adopted run. Moving the supervisor in (rather than a
    /// bare `Run`) is what makes that ordering the only representable one.
    pub(crate) supervisor: SessionSupervisor<'a>,
    /// `environment::workspace::WorkspaceOwner::session`.
    pub(crate) session: SessionRef,
    /// `environment::workspace::WorkspaceOwner::channel`: the launch, the TLS
    /// acceptor and the qualified locator publication produced.
    pub(crate) channel: PreparedHostChannel,
    /// `environment::workspace::WorkspaceOwner::serving`: the validated
    /// negotiation, fence, pin, host checkout and enrolment facts this session
    /// serves with.
    pub(crate) serving: super::ServingFacts,
    /// The activation's own effect context, retained rather than borrowed. The
    /// environment context is local to the activation and goes away when its
    /// call returns; this handle keeps the same operation and cancellation
    /// state, so the serving owner never needs a `'static` reference or a
    /// self-referential leak.
    pub(crate) effect: Arc<EffectContext>,
    /// The observation context derived from that same operation.
    pub(crate) observe: Arc<ObserveContext>,
    /// `environment::workspace::WorkspaceOwner::root`: the environment's cached
    /// state root, which the dispatcher fences every request against.
    pub(crate) root: &'a StateRoot,
}

/// The producer's frozen owner, moved in whole.
///
/// `environment::workspace::WorkspaceUp::Selected(Box<WorkspaceOwner>)` is the
/// one value the activation emits, after its environment commit, CAS receipt and
/// `selection_switched` record. This conversion is a literal field move: no run
/// or listener is rebuilt, re-adopted or dropped, and the activation must not
/// keep its own copies once it has handed the owner over.
impl From<crate::runner::environment::workspace::WorkspaceOwner>
    for OwnedWorkspaceChannel<'static>
{
    fn from(owner: crate::runner::environment::workspace::WorkspaceOwner) -> Self {
        Self {
            supervisor: owner.supervisor,
            session: owner.session,
            channel: owner.channel,
            serving: owner.serving,
            effect: owner.effect,
            observe: owner.observe,
            root: owner.root,
        }
    }
}

/// Why the frozen channel was not adopted for serving.
#[derive(Debug)]
pub(crate) enum AdoptError {
    /// The run does not carry this session's custody, or the kernel refused the
    /// adoption record.
    Custody(RunError),
    /// The prepared channel was published for another session. A listener is
    /// only ever served for the session it was qualified and published for.
    WrongSession,
    /// The publication's validated serving facts disagree with the prepared
    /// channel this owner was handed (pin or session). Serving under either
    /// would serve authority the publication never validated.
    Facts,
}

impl std::fmt::Display for AdoptError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Custody(error) => write!(formatter, "{error}"),
            Self::WrongSession => formatter.write_str(
                "the prepared workspace channel belongs to another session; it is not served",
            ),
            Self::Facts => formatter.write_str(
                "the validated serving facts disagree with the prepared channel; it is not served",
            ),
        }
    }
}

impl From<RunError> for AdoptError {
    fn from(error: RunError) -> Self {
        Self::Custody(error)
    }
}

/// The listener, key and advertised locator this owner holds. Dropping it stops
/// the listener and wipes the key, so nothing outlives the owner even if the
/// serving loop returns early.
#[derive(Debug, Default)]
struct ChannelCustody {
    launch: Option<SessionLaunch>,
    locator: Option<QualifiedHostLocator>,
}

impl ChannelCustody {
    fn stop(&mut self) {
        // A stopped listener publishes nothing, so the locator goes with it.
        self.locator = None;
        if let Some(mut launch) = self.launch.take() {
            launch.stop();
        }
    }
}

impl Drop for ChannelCustody {
    fn drop(&mut self) {
        self.stop();
    }
}

impl<'a, Ad, Rs, Ob, Id> ServingOwner<'a, ServerTlsTransport, HostChannelAcceptor, Ad, Rs, Ob, Id>
where
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    /// Adopt the frozen activation's owned channel and take its serving
    /// custody.
    ///
    /// This is the consuming entry the environment calls once with the values
    /// it committed: the adopted run (the kernel's durable custody record, which
    /// the prepared channel required), the session identity it published and the
    /// prepared channel (`launch`, `acceptor`, `QualifiedHostLocator`). The
    /// returned owner retains all of them, stops the listener and wipes the key
    /// when it is dropped or stopped, and durably settles the run through
    /// [`Self::settle`].
    ///
    /// Refuses a prepared channel published for another session rather than
    /// serving a listener it did not qualify.
    pub(crate) fn adopt_owned(
        owned: OwnedWorkspaceChannel<'a>,
        server: channel::Server<Ad, Rs, Ob, Id>,
    ) -> Result<Self, AdoptError> {
        let OwnedWorkspaceChannel {
            supervisor,
            session,
            serving,
            channel: prepared,
            effect,
            observe,
            root,
        } = owned;
        let PreparedHostChannel {
            launch,
            acceptor,
            locator,
        } = prepared;
        let _ = &serving;
        if locator.session().run != session.run()
            || locator.session().generation != session.generation()
        {
            return Err(AdoptError::WrongSession);
        }
        let mut owner = Self::new(supervisor, launch, acceptor, server, effect, observe, root);
        owner.custody.locator = Some(locator);
        owner.serving = Some(serving);
        Ok(owner)
    }
}

impl<'a>
    ServingOwner<
        'a,
        ServerTlsTransport,
        HostChannelAcceptor,
        super::host_dispatch::HostDispatch,
        super::host_dispatch::HostDispatch,
        super::host_dispatch::HostDispatch,
        super::host_dispatch::HostDispatch,
    >
{
    /// Adopt the activation's owned channel with the production dispatcher.
    ///
    /// The server is built only from the publication's validated facts: the
    /// host negotiation and fence it froze, the pinned server identity, the
    /// session grant book seeded with the enrolment credential, and the
    /// engine-owned resolver/admission/observer/id seams. A locator that does
    /// not name the same session and pin refuses before anything is served.
    pub(crate) fn adopt_production(owned: OwnedWorkspaceChannel<'a>) -> Result<Self, AdoptError> {
        let OwnedWorkspaceChannel {
            supervisor,
            session,
            channel: prepared,
            serving,
            effect,
            observe,
            root,
        } = owned;
        let PreparedHostChannel {
            launch,
            acceptor,
            locator,
        } = prepared;
        if locator.session().run != session.run()
            || locator.session().generation != session.generation()
            || locator.bound().lease().pin() != &serving.pin
            || locator.session().run != serving.fence.session.run
            || locator.session().generation != serving.fence.session.generation
        {
            return Err(AdoptError::Facts);
        }
        let (server, admitted) = super::host_dispatch::production_server(&serving, root.clone());
        let mut owner = Self::new(supervisor, launch, acceptor, server, effect, observe, root);
        owner.custody.locator = Some(locator);
        owner.serving = Some(serving);
        owner.admitted = Some(admitted);
        Ok(owner)
    }
}

/// One detached session's serving owner.
///
/// The adopted supervisor is the session's custody. The launch is the listener
/// and key it published. Dropping or [`ServingOwner::stop`]ping this owner
/// stops the listener and wipes the key, so neither outlives it, and a
/// connection is only accepted after [`SessionLaunch::serve`] accepted the
/// adopted run.
pub(crate) struct ServingOwner<'a, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    supervisor: SessionSupervisor<'a>,
    custody: ChannelCustody,
    acceptor: A,
    server: channel::Server<Ad, Rs, Ob, Id>,
    cx: Arc<EffectContext>,
    observe: Arc<ObserveContext>,
    root: &'a StateRoot,
    /// The validated serving facts this session was published with. Retained
    /// for the owner's whole life so the session's server, grants and
    /// enrolment secret die with it.
    serving: Option<super::ServingFacts>,
    /// The owning clients of the runs this session admitted over its channel.
    /// Dropping them is owner loss: their runs stop instead of outliving the
    /// session that served them.
    admitted: Option<super::host_dispatch::RetainedRuns>,
    served: usize,
    _transport: PhantomData<fn() -> T>,
}

impl<'a, T, A, Ad, Rs, Ob, Id> ServingOwner<'a, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    /// The advertised locator this owner's listener published, while it holds
    /// one. Publication and the guest's dial are one fact (§8.11).
    pub(crate) fn locator(&self) -> Option<&QualifiedHostLocator> {
        self.custody.locator.as_ref()
    }

    /// Durable settlement: stop serving, release the listener and key, and
    /// settle the adopted run so the session's own close transaction records
    /// the outcome.
    pub(crate) fn settle(
        mut self,
        cleanup: &effigy_kernel::ctx::CleanupContext,
        adapter: &mut impl effigy_kernel::run::SettlementAdapter,
    ) -> Result<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>, RunError> {
        self.stop();
        self.supervisor.settle(cleanup, adapter)
    }

    /// Durable settlement that names why the session ended (owner ruling
    /// 2026-10-31): the session's own run records the already-typed cause through
    /// the kernel's `Run::interrupt` path *before* its one close transaction, so
    /// the settled outcome and cleanup say owner loss or listener failure instead
    /// of an unrecorded default.
    ///
    /// Only the causes a serving owner can reach are mapped; every other cause
    /// settles exactly as [`Self::settle`] does. A run that already settled
    /// refuses a second settlement at the kernel, as it always has.
    pub(crate) fn settle_with(
        mut self,
        cause: crate::runner::workspace_client::supervisor::RetirementCause,
        cleanup: &effigy_kernel::ctx::CleanupContext,
        adapter: &mut impl effigy_kernel::run::SettlementAdapter,
    ) -> Result<effigy_kernel::outcome::Outcome<effigy_kernel::run::Settled>, RunError> {
        use crate::runner::workspace_client::supervisor::RetirementCause;
        self.stop();
        let workload = match cause {
            RetirementCause::OwnerLost => {
                Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::OwnerLost,
                    signal: None,
                })
            }
            RetirementCause::ListenerFailed => {
                Some(effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                    cause: effigy_kernel::ctx::StopCause::ListenerFailed,
                    signal: None,
                })
            }
            RetirementCause::Settled | RetirementCause::Stopped | RetirementCause::Down => None,
        };
        let mut supervisor = self.supervisor;
        if let Some(workload) = workload {
            supervisor.interrupt(cleanup, workload)?;
        }
        supervisor.settle(cleanup, adapter)
    }

    /// Take the session's serving custody. The supervisor must be the adopted
    /// owner of the run the launch belongs to; the kernel refuses anything else
    /// at the first [`SessionLaunch::serve`].
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        supervisor: SessionSupervisor<'a>,
        launch: SessionLaunch,
        acceptor: A,
        server: channel::Server<Ad, Rs, Ob, Id>,
        cx: Arc<EffectContext>,
        observe: Arc<ObserveContext>,
        root: &'a StateRoot,
    ) -> Self {
        Self {
            supervisor,
            custody: ChannelCustody {
                launch: Some(launch),
                locator: None,
            },
            acceptor,
            server,
            cx,
            observe,
            root,
            serving: None,
            admitted: None,
            served: 0,
            _transport: PhantomData,
        }
    }

    /// The listener this owner still holds, while it holds one.
    pub(crate) fn launch(&self) -> Option<&SessionLaunch> {
        self.custody.launch.as_ref()
    }

    /// Whether the session's key is still in custody.
    pub(crate) fn key_held(&self) -> bool {
        self.custody
            .launch
            .as_ref()
            .is_some_and(|launch| launch.key_held())
    }

    /// Whether the launch may dispatch on its listener for this run.
    pub(crate) fn serving(&self) -> bool {
        self.custody
            .launch
            .as_ref()
            .is_some_and(|launch| launch.serving(&self.supervisor))
    }

    /// Connections this owner has dispatched.
    pub(crate) fn served(&self) -> usize {
        self.served
    }

    /// Serve one pending connection, if one is waiting.
    ///
    /// The launch is marked serving for the adopted run *before* the accept, so
    /// a connection is never taken for a session nothing owns or a locator that
    /// was never published. `Ok(None)` means nothing was waiting; the caller's
    /// cadence decides when to ask again.
    pub(crate) fn serve_pending(
        &mut self,
        streams: &mut impl Streams,
    ) -> Result<Option<Served>, ServeError> {
        {
            let launch = self.custody.launch.as_mut().ok_or(ServeError::Stopped)?;
            launch.serve(&self.supervisor)?;
        }
        let Some(mut transport) = self.acceptor.accept(
            self.custody.launch.as_ref().ok_or(ServeError::Stopped)?,
            &self.supervisor,
        )?
        else {
            return Ok(None);
        };
        let served = self.server.serve(
            &mut transport,
            self.cx.as_ref(),
            self.observe.as_ref(),
            self.root,
            streams,
        )?;
        self.served += 1;
        Ok(Some(served))
    }

    /// Poll for connections until `stop` is set, asking at most every `tick`.
    /// The supervisor owns this cadence and the stop decision; this loop never
    /// waits on a connection and never keeps one alive past its own end.
    pub(crate) fn run(
        &mut self,
        streams: &mut impl Streams,
        tick: Duration,
        stop: &AtomicBool,
    ) -> Result<usize, ServeError> {
        while !stop.load(Ordering::SeqCst) {
            match self.serve_pending(streams) {
                Ok(Some(_)) => continue,
                Ok(None) => std::thread::sleep(tick.max(Duration::from_millis(1))),
                // A refused connection is over; the session keeps serving.
                Err(error) if !error.is_terminal() => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(self.served)
    }

    /// Stop the listener and wipe the key now, keeping the session run's
    /// custody with its supervisor. Nothing this owner published is servable
    /// afterwards.
    pub(crate) fn stop(&mut self) {
        self.custody.stop();
    }

    /// Release the launch's custody and stop serving.
    pub(crate) fn release(mut self) {
        self.stop();
    }
}

impl<T, A, Ad, Rs, Ob, Id> std::fmt::Debug for ServingOwner<'_, T, A, Ad, Rs, Ob, Id>
where
    T: Transport,
    A: ChannelAccept<Transport = T>,
    Ad: channel::Admission,
    Rs: channel::Resolver,
    Ob: channel::Observer,
    Id: channel::RunIds,
{
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServingOwner")
            .field("serving", &self.serving())
            .field("served", &self.served)
            .finish_non_exhaustive()
    }
}
