//! The environment-owned workspace session (052 §8.7) on top of the kernel
//! workspace seam (§8.11).
//!
//! An environment that declares an operator workspace has exactly one
//! `Detached` workspace session run. This module owns that session's identity:
//! which run is the environment's one session, what a front end prints for it,
//! and the down a session requested from inside. The replay-safe admission
//! record, the durable client attachments, the bounded stream cursor and the
//! bounded control path all belong to the kernel workspace seam in
//! `crate::run::workspace` (lane A's channel builds on the same records), so a
//! session consumes them instead of keeping a second copy. `crate::remote::{
//! session, stream}` remains remote *workload* custody and is never host
//! session authority.
//!
//! Nothing here spawns a process, binds a socket, signals a group or settles
//! another run: the session run is admitted through `Run::begin`, a child
//! through `Run::begin_child`, control travels as a journalled `run::request`,
//! and a down takes its permit through `run::Attachments`.
//!
//! [`WorkspaceActivationReservation`] is the one exception to "a session is
//! recorded when it is admitted": a successor environment generation E must be
//! known before the session is admitted, because the canonical gate order takes
//! `Gate(Task)` before `Gate(Env)`. It reserves E durably, the session is
//! admitted against E under `Gate(Task)` alone, and the activation run at E
//! claims the reservation and later commits the environment's session mapping,
//! `active(E)` and the session's `Borrow` attachment in one transaction. E and
//! the session's own owner generation S are always separate fences.
//!
//! The successor overlap is deliberately narrow: only the exact frozen workspace
//! session claim admits beside its preserved predecessor. Both keep the same
//! logical Task owner; the successor holds a private, run-specific liveness slot
//! whose durable binding grants no generic Task admission or target authority.
//! The Task's current fence stays unchanged until the atomic activation commit.
//! Run-bound lifecycle checks let earlier sessions finish their own journals
//! afterwards. Generic K5 admission refuses while these sessions remain live or
//! unsettled. Nothing stops a predecessor, retargets its selected bytes or binds
//! its listener. Cancellation/recovery takes only the exact successor's slot.

mod activation;
mod claims;
mod down;
mod guest_context;
mod guest_stage;
mod selection;
mod serving;
mod session;
mod transition;
pub(crate) use transition::{check_intent_target, check_run_fence, resume_facts};

#[cfg(test)]
mod tests;

pub(crate) use activation::{
    activation_evidence, activation_reader, owner_identity, read_reservation,
    recoverable_reservation, require_reservation,
};
pub use activation::{
    ActivationDecision, ActivationRefusal, ActivationReservationId, ActivationReservationRecord,
    ActivationReservationState, ArtifactEvidence, ContainerIdentity, PublishInputs, SessionBinding,
    TargetCandidate, TargetEvidence, WorkspaceActivationReservation, WorkspacePublicationReceipt,
};
pub(crate) use claims::{
    activation_plan, check_session_owner, check_staging_attempt, refuse_bound_reservation_recovery,
    refuse_reserved_generation, verify_reserved_owner_dead,
};
pub use claims::{
    ActivationClaim, ActivationPlan, CommittedActivation, EnvironmentClaim, SessionClaim,
};
pub use down::{DownChildren, DownCoordination, DownHolder, DownStart};
pub use guest_context::{GuestContextInputs, GuestPaths, GuestRoute, GuestSourcePin};
pub use guest_stage::{
    BoundPlatform, GuestBinaryReceipt, GuestBinaryStage, ImagePlatform, LoaderFamily,
};
pub use selection::{
    committed_selection, selection_abandoned, selection_recovered, unfinished_selection,
    AdvertisedLocator, CredentialSelection, GuestObject, GuestSelection, SelectionLiveness,
    SelectionObjects, SelectionObligation, SelectionReceipt, SelectionState,
    SelectionSwitchReceipt, SelectorChange, SelectorReadback, SelectorReader,
    WorkspaceContextDocument, WorkspaceSessionBinding, BINARY_SELECTOR, CONTEXT_SELECTOR,
    ENTRY_SELECTOR, WORKSPACE_CONTEXT_SCHEMA,
};
pub use serving::{
    ChannelBound, ChannelListener, GuestLocatorChallenge, GuestLocatorEvidence, GuestLocatorTarget,
    HostChannelListener, ListenerPhase, ServingKey, ServingLease, ServingProbe, ServingRefusal,
    SessionLaunch, SessionOwner, TargetProbe,
};
pub(crate) use session::{
    decode_owner, live_session_row, next_generation, preserve_active_predecessor,
    preserved_environment, refuse_session_overlap, request_stop, run_needs_settlement,
    session_evidence, session_of_run, session_run_gate, session_slot, SessionRow,
};
pub use session::{phase, OwnerLoss, SessionHandle, SessionRef, SessionSupervisor, TerminalKind};

use crate::ctx::{CleanupContext, EffectContext, ObserveContext, StopCause};
use crate::gate::{Gate, Wait};
use crate::id::{Digest, Generation, ResolvedInvocation, ResourceId, ResourceKind, RunId};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use crate::run::{
    self, AttachMode, Attachments, BeginOpts, Fence, Phase, Release, Run, RunError,
    SettlementAdapter,
};
use crate::store::{Store, StoreReader, Txn};
use crate::supervise::Placement;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
