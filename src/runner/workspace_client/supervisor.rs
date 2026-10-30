//! Durable serving supervision of one selected container workspace
//! (contract 052 §8.3, §8.7, §8.11; effigy#507).
//!
//! A selected workspace's session owns a listener, a memory-only key, the
//! grants its validated facts seeded and the clients it admitted. Those are the
//! session's custody, not the asking command's: the kernel keeps them in the
//! adopted [`SessionSupervisor`] and its [`SessionLaunch`], and releases them
//! only when that session settles or its owner is abandoned. What was missing
//! was the owner of that custody: the container-up caller adopted the
//! [`super::serving::ServingOwner`] in its own process, returned, and dropped
//! it, which stops the listener and wipes the key while the committed selection
//! still says the session is serving.
//!
//! This module is that owner. It binds the #452 handoff signature
//! ([`ServingHandoff`]) to a supervision value that outlives the call which
//! produced it ([`ServiceOwner`]), decides when serving ends from the session's
//! own stop and owner liveness (`ServingEnd`), and settles the session exactly
//! once through the kernel's close transaction ([`ServiceOwner::retire`]).
//! Nothing here creates a run, a session, a listener, a key, a grant or a
//! credential: every one of those is the activation's, moved in whole.
//!
//! Two boundaries are deliberate:
//!
//! - **Handoff is not adoption.** [`ServingHandoff`] is a value a caller can
//!   hold and must dispose of: [`ServiceOwner::adopt`] takes it into service,
//!   and [`ServingHandoff::interrupted`] closes it without serving. Dropping it
//!   is the kernel's own release (the launch's custody closes the socket and
//!   wipes the key); nothing in this module claims that release as custody, and
//!   nothing here resurrects it.
//! - **Discovery is durable, never process-local.** [`discover`] names a
//!   session and its published lease from the checkout journal alone, under
//!   durable session identity (§8.7). There is no registry singleton here, so a
//!   recovery or down driver in another command, process or run finds the same
//!   session by the same facts.
//!
//! The process that hosts the supervision is the session run's own detached
//! supervisor (contract §5.8, §8.3: "the listener lives exactly as long as that
//! run"). [`ServiceOwner::serve`] is that supervisor's loop and
//! [`ServiceOwner::retire`] is its settle; a supervisor re-execs before it
//! builds the listener, so no live listener or key ever has to cross a process
//! boundary, and a supervisor that dies takes its socket and key with it.

#[cfg(test)]
#[path = "supervisor_tests.rs"]
mod tests;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use effigy_kernel::ctx::CleanupContext;
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::Outcome;
use effigy_kernel::root::StateRoot;
use effigy_kernel::run::{Phase, RunError, Settled, SettlementAdapter};
use effigy_kernel::store::Store;
use effigy_kernel::workspace_channel::Streams;
use effigy_kernel::workspace_session::{
    phase, unfinished_selection, SelectionObligation, SelectionReceipt, ServingLease, SessionRef,
};

use super::host_dispatch::HostDispatch;
use super::serving::{AdoptError, OwnedWorkspaceChannel, ServeError, ServingOwner};
use crate::runner::environment::workspace::WorkspaceOwner;
use crate::runner::workspace_transport::listener::{HostChannelAcceptor, ServerTlsTransport};

/// The adopted supervisor's own serving owner: the production dispatcher, the
/// session's TLS listener and the exact values the activation committed.
pub(crate) type ChannelOwner<'a> = ServingOwner<
    'a,
    ServerTlsTransport,
    HostChannelAcceptor,
    HostDispatch,
    HostDispatch,
    HostDispatch,
    HostDispatch,
>;

/// Why a session's custody was not handed to service.
#[derive(Debug)]
pub(crate) enum HandoffRefusal {
    /// The launch never published, or published a lease other than the one its
    /// commit recorded. Serving under either would serve authority no
    /// publication validated.
    Unpublished,
    /// The commit belongs to another session than the one this custody owns.
    WrongSession,
    /// The launch no longer holds the session's key, so it cannot serve.
    KeyReleased,
}

impl std::fmt::Display for HandoffRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unpublished => {
                "the workspace session has not published the lease its commit recorded"
            }
            Self::WrongSession => {
                "the committed selection belongs to another workspace session than this custody"
            }
            Self::KeyReleased => "the workspace session no longer holds its serving key",
        })
    }
}

/// One committed selected session's exact custody, handed to service.
///
/// The activation produces it after its environment commit, its publication
/// receipt and its `selection_switched` record, from the owner it emitted
/// (`WorkspaceUp::Selected`) and the obligation it committed. Every field is a
/// move: no run, listener, key or fact is rebuilt, re-adopted or copied here,
/// and the caller must not keep its own copies once it has handed this over.
pub(crate) struct ServingHandoff<'a> {
    owner: OwnedWorkspaceChannel<'a>,
    session: SessionRef,
    lease: ServingLease,
}

impl std::fmt::Debug for ServingHandoff<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServingHandoff")
            .field("session", &self.session)
            .field("lease", &self.lease)
            .finish_non_exhaustive()
    }
}

impl ServingHandoff<'static> {
    /// Hand off the selected session the commit recorded.
    ///
    /// This is the typed seam the container-up caller binds: only the committed
    /// selected session may hand off, and the launch must have published
    /// exactly the lease the receipt froze (`SelectionReceipt::serving`).
    pub(crate) fn committed(
        owner: WorkspaceOwner,
        obligation: &SelectionObligation,
    ) -> Result<Self, HandoffRefusal> {
        if obligation.session_run() != owner.session.run() {
            return Err(HandoffRefusal::WrongSession);
        }
        Self::published(owner, obligation.receipt())
    }

    /// The same handoff, from the receipt that was committed.
    pub(crate) fn published(
        owner: WorkspaceOwner,
        receipt: &SelectionReceipt,
    ) -> Result<Self, HandoffRefusal> {
        Self::validated(owner.into(), receipt.serving())
    }
}

impl<'a> ServingHandoff<'a> {
    /// The one validation both entries share: the launch's own published lease
    /// must be the committed lease, for this session, with its key in custody.
    fn validated(
        owner: OwnedWorkspaceChannel<'a>,
        committed: &ServingLease,
    ) -> Result<Self, HandoffRefusal> {
        if committed.session().run != owner.session.run()
            || committed.session().generation != owner.session.generation()
        {
            return Err(HandoffRefusal::WrongSession);
        }
        if !owner.channel.launch.key_held() {
            return Err(HandoffRefusal::KeyReleased);
        }
        // A launch that published nothing, or published another lease, cannot
        // serve this commit: the locator a guest dials and the lease a commit
        // recorded are one fact (§8.11).
        match owner.channel.launch.lease() {
            Some(published) if published == *committed => {}
            _ => return Err(HandoffRefusal::Unpublished),
        }
        let session = owner.session.clone();
        Ok(Self {
            owner,
            session,
            lease: committed.clone(),
        })
    }

    /// The session this custody belongs to.
    pub(crate) fn session(&self) -> &SessionRef {
        &self.session
    }

    /// The lease this custody serves.
    pub(crate) fn lease(&self) -> &ServingLease {
        &self.lease
    }

    /// Whether the listener is still bound and its key still held.
    pub(crate) fn held(&self) -> bool {
        self.owner.channel.launch.key_held() && self.owner.channel.launch.endpoint().is_some()
    }

    /// An interrupted handoff: the caller gave the session up before service
    /// adopted it. The prepared candidate is closed — socket closed, key wiped,
    /// nothing published as served — and the report says exactly that. This is
    /// not custody: nothing outlives the call.
    pub(crate) fn interrupted(self) -> Interrupted {
        let mut launch = self.owner.channel.launch;
        launch.stop();
        Interrupted {
            session: self.session,
            socket_closed: launch.endpoint().is_none(),
            key_released: !launch.key_held(),
        }
    }
}

/// What an interrupted handoff closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Interrupted {
    /// The committed session whose candidate was stopped.
    pub session: SessionRef,
    /// The listener is stopped, so its locator can never be reused.
    pub socket_closed: bool,
    /// The session's key bytes were wiped.
    pub key_released: bool,
}

/// Why the supervision stopped serving. Each ends the loop exactly once and
/// each is the settle's own cause; none is a retry.
#[derive(Debug)]
pub(crate) enum ServingEnd {
    /// The session asked to stop: its own stop request, a `down`, or its owner
    /// releasing it.
    Stopped,
    /// The owning client went away without a detach: the kernel's owner-loss
    /// path, which stops and settles the run it owned.
    OwnerLost,
    /// The session's custody or listener failed terminally. A refused
    /// connection is not one of these; it ends that connection only.
    Failed(ServeError),
}

impl std::fmt::Display for ServingEnd {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stopped => formatter.write_str("the workspace session stopped serving"),
            Self::OwnerLost => formatter.write_str("the workspace session's owner was lost"),
            Self::Failed(error) => write!(formatter, "{error}"),
        }
    }
}

/// The detached serving supervision of one committed session.
///
/// It retains the exact adopted supervisor/run, the prepared listener and key,
/// the validated facts (and so the session's grant book and enrolment secret)
/// and every client the session admitted. [`Self::serve`] is its loop and
/// [`Self::retire`] is its one settle.
#[derive(Debug)]
pub(crate) struct ServiceOwner<'a> {
    owner: ChannelOwner<'a>,
    session: SessionRef,
    lease: ServingLease,
    served: usize,
}

impl<'a> ServiceOwner<'a> {
    /// Take the committed session's custody into service with the production
    /// dispatcher: one `HostDispatch` answers all four kernel seams, and the
    /// server is built only from the publication's validated facts.
    pub(crate) fn adopt(handoff: ServingHandoff<'a>) -> Result<Self, AdoptError> {
        let ServingHandoff {
            owner,
            session,
            lease,
        } = handoff;
        // A refused adoption closes the candidate: `adopt_production` consumed
        // the prepared channel, whose custody stops the listener and wipes the
        // key when it is dropped.
        let adopted = ServingOwner::adopt_production(owner)?;
        Self::retained(adopted, session, lease)
    }

    /// The same custody on an injected server, so the proofs can exercise the
    /// supervision without the production dispatcher.
    pub(crate) fn retained(
        owner: ChannelOwner<'a>,
        session: SessionRef,
        lease: ServingLease,
    ) -> Result<Self, AdoptError> {
        // Adoption is only ever the committed lease: a server that published
        // another one is not this session's service.
        if owner.locator().map(|locator| locator.bound().lease()) != Some(&lease) {
            let mut owner = owner;
            owner.stop();
            return Err(AdoptError::WrongSession);
        }
        Ok(Self {
            owner,
            session,
            lease,
            served: 0,
        })
    }

    /// Adopt a committed handoff on an injected server: the proofs use it to
    /// measure the same supervision over scripted connections.
    pub(crate) fn adopt_on(
        handoff: ServingHandoff<'a>,
        server: effigy_kernel::workspace_channel::Server<
            super::host_dispatch::HostDispatch,
            super::host_dispatch::HostDispatch,
            super::host_dispatch::HostDispatch,
            super::host_dispatch::HostDispatch,
        >,
    ) -> Result<Self, AdoptError> {
        let ServingHandoff {
            owner,
            session,
            lease,
        } = handoff;
        let adopted = ServingOwner::adopt_owned(owner, server)?;
        Self::retained(adopted, session, lease)
    }

    /// The session in service.
    pub(crate) fn session(&self) -> &SessionRef {
        &self.session
    }

    /// The lease in service: one commit, one locator, one key.
    pub(crate) fn lease(&self) -> &ServingLease {
        &self.lease
    }

    /// Whether the session's key is still in custody.
    pub(crate) fn key_held(&self) -> bool {
        self.owner.key_held()
    }

    /// Whether the session run still authorises the channel (adopted, bound,
    /// published and not stopping).
    pub(crate) fn serving(&self) -> bool {
        self.owner.serving()
    }

    /// Connections this supervision dispatched.
    pub(crate) fn served(&self) -> usize {
        self.served
    }

    /// Stop serving now: the session's own stop request, a `down`, or the
    /// supervisor's own shutdown. The listener stops and the key is wiped, so
    /// nothing this session published is servable again; the run keeps its
    /// custody until [`Self::retire`] settles it.
    pub(crate) fn stop(&mut self) {
        self.owner.stop();
    }

    /// Serve at most one pending connection. `Ok(true)` means one was accepted
    /// and completed; `Ok(false)` means nothing was waiting. A connection that
    /// was taken and then failed its own turn is `Err` for that connection
    /// only, and it is the caller that decides whether the failure is terminal
    /// ([`ServeError::is_terminal`]).
    pub(crate) fn serve_pending(&mut self, streams: &mut impl Streams) -> Result<bool, ServeError> {
        match self.owner.serve_pending(streams) {
            Ok(served) => {
                if served.is_some() {
                    self.served += 1;
                }
                Ok(served.is_some())
            }
            Err(error) => {
                // A transport or channel failure is a connection this
                // supervision took and then ended; a refused custody or a
                // released launch is not a connection at all.
                if matches!(error, ServeError::Channel(_) | ServeError::Transport(_)) {
                    self.served += 1;
                }
                Err(error)
            }
        }
    }

    /// Serve the session until `stop` is set, the owner is lost, or the
    /// session's custody refuses.
    ///
    /// The launch is marked serving for the adopted run *before* each accept, so
    /// no connection is taken for a session nothing owns or a locator that was
    /// never published. A refused connection ends that connection only and the
    /// session keeps serving; `Ok(None)` means nothing was waiting, and `tick`
    /// bounds the next ask.
    pub(crate) fn serve(
        &mut self,
        streams: &mut impl Streams,
        tick: Duration,
        stop: &AtomicBool,
        owner_lost: &mut dyn FnMut() -> bool,
    ) -> ServingEnd {
        loop {
            if stop.load(Ordering::SeqCst) {
                return ServingEnd::Stopped;
            }
            if owner_lost() {
                return ServingEnd::OwnerLost;
            }
            match self.serve_pending(streams) {
                Ok(true) => {}
                Ok(false) => std::thread::sleep(tick.max(Duration::from_millis(1))),
                Err(error) if error.is_terminal() => return ServingEnd::Failed(error),
                // One refused connection is over; the session keeps serving.
                Err(_) => {}
            }
        }
    }

    /// Retire the session: stop serving, release the listener and key, and
    /// settle the adopted run through its own close transaction, exactly once.
    ///
    /// The release is ordered the way the kernel's contract requires: the
    /// listener stops (so the published locator is no longer servable), the key
    /// is wiped, the grant book and the admitted clients die with the serving
    /// facts and the owner, and only then does the session run settle. A settle
    /// that fails keeps the release already made and reports the kernel's error
    /// rather than claiming completion.
    pub(crate) fn retire(
        self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Retired, RunError> {
        let served = self.served;
        let Self {
            mut owner,
            session,
            lease,
            ..
        } = self;
        owner.stop();
        let outcome = owner.settle(cleanup, adapter)?;
        Ok(Retired {
            session,
            lease,
            outcome,
            served,
        })
    }

    /// Give the session up without settling it. The listener stops and the key
    /// is wiped; the run's settlement stays with whoever owns that run.
    pub(crate) fn abandon(self) -> Abandoned {
        let mut owner = self.owner;
        owner.stop();
        Abandoned {
            session: self.session,
            served: self.served,
        }
    }
}

/// One settled session.
#[derive(Debug)]
pub(crate) struct Retired {
    /// The session that settled.
    pub session: SessionRef,
    /// The lease it served, for the record: it is never servable again.
    pub lease: ServingLease,
    /// The kernel's outcome for the session run.
    pub outcome: Outcome<Settled>,
    /// Connections dispatched before settlement.
    pub served: usize,
}

impl Retired {
    /// Whether the settled run reports complete cleanup.
    pub(crate) fn complete(&self) -> bool {
        self.outcome.cleanup() == effigy_kernel::outcome::CleanupOutcome::Complete
    }
}

/// One session given up without settlement.
#[derive(Debug)]
pub(crate) struct Abandoned {
    pub session: SessionRef,
    pub served: usize,
}

/// One session found from durable identity alone.
///
/// `discover` is what a recovery or down driver in another run, command or
/// process reads: the environment's committed session and its run's durable
/// phase, plus the lease of a commit that has not finished switching. It is a
/// record, not a handle: holding it grants nothing and serving from it is
/// impossible.
#[derive(Debug)]
pub(crate) struct ServingFacility {
    /// The environment's committed workspace session.
    pub session: SessionRef,
    /// The session run's durable phase.
    pub phase: Phase,
    /// The lease of this session's still-unfinished commit, when it has one.
    ///
    /// A committed selection that already switched leaves no unfinished record,
    /// so `None` here does not mean "not serving": this kernel has no public
    /// reader for a switched selection's lease, and a driver must read the
    /// session's phase rather than infer an absence from this field.
    pub unfinished: Option<ServingLease>,
}

impl ServingFacility {
    /// Whether the session run is still live, so a driver may ask it to stop or
    /// may complete its unfinished commit. It is a question to confirm, never
    /// authority to serve.
    pub(crate) fn live(&self) -> bool {
        !matches!(self.phase, Phase::Stopping | Phase::Closed)
    }
}

/// Find the environment's committed workspace session and what its commit
/// published, from the checkout journal alone.
///
/// Absent means this environment has no committed workspace session; Unknown
/// means the journal or one of its records could not be read, which is not an
/// absence. Nothing here consults a process-local registry, so the answer is the
/// same in the command that brought the workspace up and in any later one.
pub(crate) fn discover(
    observe: &effigy_kernel::ctx::ObserveContext,
    root: &StateRoot,
    environment: &effigy_kernel::id::ResourceId,
) -> Observed<Option<ServingFacility>> {
    let reader = match Store::read(observe, root) {
        Observed::Verified(reader, evidence) => (reader, evidence),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(absent) => return Observed::Absent(absent),
    };
    let (reader, evidence) = reader;
    let session = match SessionRef::resolve(&reader, environment) {
        Observed::Verified(Some(session), _) => session,
        Observed::Verified(None, _) => return Observed::Verified(None, evidence),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(absent) => return Observed::Absent(absent),
    };
    let phase = match phase(&reader, session.run()) {
        Observed::Verified(phase, _) => phase,
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(absent) => return Observed::Absent(absent),
    };
    let pending = match unfinished_selection(&reader, environment) {
        Observed::Verified(pending, _) => pending,
        Observed::Unknown(reason) => return Observed::Unknown(reason),
        Observed::Absent(_) => None,
    };
    let unfinished = pending.as_ref().map(|obligation| {
        let receipt: &SelectionReceipt = obligation.receipt();
        receipt.serving().clone()
    });
    Observed::Verified(
        Some(ServingFacility {
            session,
            phase,
            unfinished,
        }),
        evidence,
    )
}
