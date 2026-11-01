# Reserved workspace activation: integration interface

This is the interface for the three concurrent #512 implementation slices. It
is a proposed implementation boundary, not evidence that publication, recovery,
or native qualification works. Contracts [052](052-runtime-kernel-contract.md)
and [053](053-environment-reconciler-contract.md) govern behavior. Delete or
fold this coordination artifact into those owners when the slices integrate.

## Ownership and frozen inputs

One detached supervisor owns the exact `SealedPreflight`, frozen
`ReservedPublicationPlan`, original backend adapter and absolute `Instant`
deadline. It admits one `SessionSupervisor` and one reserved environment run.
Neither the CLI nor an integration helper creates a replacement owner. The
existing `BoundReservedGuest<'a>` retains the original `ReservedGuestInput`,
publication ticket, target, platform, plan, and creator receipt. Its existing
`revalidate(&mut self, &mut WorkspaceActivation<'static>, &mut dyn
EnvironmentBackend) -> Result<(), EnvError>` is presently callable only under
an already guarded owner; it is not yet parked reentry.

## Slice A: kernel owner and resume

A alone edits `crates/effigy-kernel/src/run/`, `gate.rs`,
`workspace_session.rs`, `crates/effigy-containers/src/environment/reconcile.rs`,
`workspace_activation.rs`, their tests and environment exports. Existing interfaces:

```rust
Run::park_workspace(self, &WorkspaceActivationReservation, Instant)
    -> Result<ParkedWorkspaceRun<'r>, (RunError, Box<Run<'r>>)>;
ParkedWorkspaceRun::reacquire(self)
    -> Result<ReacquiredWorkspaceRun<'r>, (RunError, Box<ParkedWorkspaceRun<'r>>)>;
ParkedWorkspaceRun::settle(self, &CleanupContext, &mut impl SettlementAdapter)
    -> Result<Outcome<Settled>, (RunError, WorkspaceCleanupCustody<'r>)>;
```

`ReacquiredWorkspaceRun` intentionally does not currently return `Run` or
permit effects. A will add the following narrow **proposed** cross-crate API;
B/C must not assume it exists until A supplies it:

```rust
pub struct WorkspaceResumeFacts {
    pub reservation: ActivationReservationId,
    pub environment: ResourceId,
    pub environment_run: RunId,
    pub environment_generation: Generation,
    pub session: SessionBinding,
    pub selection: Digest,
    pub target: ContainerIdentity,
    pub platform: BoundPlatform,
    pub creator_receipt: serde_json::Value,
    pub created_ids: Vec<String>,
}
ReacquiredWorkspaceRun::view(&self) -> Result<WorkspaceResumeView<'_>, RunError>;
ReacquiredWorkspaceRun::resume(
    self,
    observed: Observed<WorkspaceResumeFacts>,
) -> Result<Run<'r>, (RunError, Box<ReacquiredWorkspaceRun<'r>>)>;
```

`WorkspaceResumeFacts` will name the exact reservation ID, environment and
run/generation, session owner/run/generation, frozen selection digest, target
container/boot/backend, and the exact journalled creator receipt and created-ID
set. A owns the concrete type and checks it against the snapshot and current
journal, original process identity, live private session slot, gate ownership,
cancellation and the original deadline before returning `Run`. `Absent`,
`Unknown`, missing or changed facts refuse. A plain boolean or a callback that
can simply return success is not a resume proof. B supplies `Observed::Verified`
only from actual bounded reobservation, never a fabricated result. If the
kernel cannot validate a field, the seam remains named fail-closed.

## Slice B: gate-free integrations and cleanup

B alone edits the **new** `src/runner/container_command/workspace_integrations.rs`
and its new tests. B does not edit `lifecycle.rs`, kernel, containers environment
or existing runner files; C owns module registration. B scaffolds against fakes
until A's concrete interface lands. Its integration surface is:

```rust
pub(in crate::runner) enum WorkspaceIntegrationStep {
    Readiness, Permissions, Secrets, Routes, HostIntegrations,
}
pub(in crate::runner) trait WorkspaceIntegrationOwner {
    fn session(&self) -> &SessionRef;
    fn deadline(&self) -> Instant;
    fn record_step(&mut self, step: WorkspaceIntegrationStep, completed: bool)
        -> Result<(), RunnerError>;
}
pub(in crate::runner) fn run_workspace_integrations(
    owner: &mut dyn WorkspaceIntegrationOwner,
    policy: &EffectiveContainerPolicy,
) -> Result<WorkspaceIntegrationOutcome, RunnerError>;
pub(in crate::runner) fn unwind_workspace_integrations(
    owner: &mut dyn WorkspaceIntegrationOwner,
    outcome: &mut WorkspaceIntegrationOutcome,
) -> Result<(), RunnerError>;
```

`WorkspaceIntegrationOutcome` is B-owned and retains the routes/listeners needed
for reverse unwind; it never contains secret values. C supplies the policy and
A's parked owner adapter, plus the bounded effect and cleanup adapter B needs.
These signatures are proposed, not yet compiled. B
runs readiness, permissions, secrets, routes and host integrations while Env is
free and before publication. Ordinary effects are idempotent/reconcilable, not
authenticated receipts; prove crash/rerun convergence, identical runtime files
and routes. Unknown cleanup leaves the reservation recoverable and reports
`integration effects unsettled; re-run up`. Never apply twice.

## Slice C: retained plan, detached owner and E2E

C alone edits `src/runner/container_command/lifecycle.rs`,
`src/runner/workspace_client/serving_host.rs`, the **new**
`src/runner/workspace_client/container_up.rs`, module registration, E2E tests,
and the `environment_workspace`/`reserved_guest` seam after A's interface lands.
C does not edit A's kernel or containers environment files or B's new file. C consumes A's park/reacquire interface and B's integration interface without
another run owner. A's planned container entrypoint is:

```rust
ParkedWorkspaceActivation::reacquire(
    self,
    observe: impl FnOnce(&WorkspaceResumeView<'_>, Instant)
        -> Observed<WorkspaceResumeFacts>,
) -> Result<WorkspaceActivation<'r>, (EnvError, Box<ParkedResumeCustody<'r>>)>;
```

`ParkedResumeCustody` preserves the exact parked or guarded owner on every
failure. The observer uses the same retained backend and `BoundReservedGuest`
to obtain actual fresh facts; the callback cannot mint evidence from cached
facts. The existing `BoundReservedGuest::revalidate` requires a guarded
`WorkspaceActivation`, so C must adapt its observation into the read-only view
rather than treating that method as already wired. This signature is proposed
and must be reconciled against A's actual landed API. Split the
existing producer in this order: bind target/platform and retain
`BoundReservedGuest`; consume activation into parked custody; run B's
integrations; reacquire and revalidate through A/B; publish using the same
input and ticket; commit/select; transfer the exact `WorkspaceOwner` to the
selected detached serving owner. The owner acknowledges only after service is
ready. Ambiguous handoff, owner/listener loss and down settle that exact run.

C proves the actual orchestration with a fake backend and a third process after
CLI exit issuing an authorized Observe over the retained listener, including
unissued-capability and failure controls. This fake is not native
qualification. Colima, Docker Desktop and Linux full-session gates each need
an explicit exact-SHA Supervisor GO. Keep the merge hold and independent
inherited-plus-delta reviews.

Run-specific `runs:status` is in progress in #512; the existing `runs:list`
seam does not establish that capability or production activation qualification.

## Apply boundary after integration (effigy#533, 2026-11-02)

The slices are integrated and merged (#659). C's producer now takes the one
ordinary apply as an injected strategy
(`environment_workspace::PrivateApply`), and the guarded Env run lends it the
real `Run`, child effect and deadline through `apply_sealed_with_owner`. The
reserved-guest proofs (`workspace_reserved_*`) supply a fake backend on that
boundary and pass 13/13; `workspace_reserved_owner_publishes_one_full_v2_through_real_commit_and_selection`
plus the eight sibling publication, revalidation, deadline and guest-write
crash proofs reach committed selection through the real owner.

The production strategy still refuses, so no workspace apply executes. The
blocker is resident Env custody: see the apply boundary section of
[053](053-environment-reconciler-contract.md). The four `#[ignore]`d serving-host
proofs in `tests/workspace_client/container_up_production.rs` need the same
capability plus the fixture apply seam, and remain #531's positive E2E.
