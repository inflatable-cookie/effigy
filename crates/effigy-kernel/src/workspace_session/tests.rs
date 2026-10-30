//! Fake-owner and fake-runtime proofs for the workspace session (§8.7).
//!
//! These tests drive the session layer a supervisor and a dispatcher use: one
//! environment-owned `Detached` session run, one identity for every front end,
//! the down a session requests from inside, and the loss a terminal reports.
//! The replay-safe admission record, the durable attachments and the bounded
//! stream cursor belong to the kernel workspace seam (`crate::run::workspace`)
//! and are proven there; a session consumes them. No engine, container,
//! gateway, socket server or live supervisor is involved: the "fake owner" is a
//! real kernel `Run` in a fixture journal, and the "fake runtime" is a
//! settlement adapter that records nothing.

use super::*;
use crate::ctx::{
    CancelToken, Deadline, EffectClass, EffectContext, Limits, OperationContext, StopCause,
};
use crate::id::{CheckoutId, CheckoutKind, ExecutionTarget, Route, Surface, TaskId};
use crate::observe::Observed;
use crate::outcome::{Invocation, WorkloadOutcome};
use crate::run::{Effect, EffectSpec, Reconciled, RequestAction, Settlement, SettlementAdapter};
use crate::store::Store;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

fn evidence() -> Evidence {
    Evidence::new("fake session adapter".into(), None).expect("fixed evidence")
}

fn context() -> EffectContext {
    OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 2,
        },
        EffectClass::Mutate,
        crate::ctx::Placement::Host,
    )
    .effects()
    .expect("host Mutate context")
}

fn cleanup(cx: &EffectContext) -> CleanupContext {
    cx.operation()
        .cleanup(Duration::from_secs(10))
        .expect("cleanup budget")
}

fn observe(cx: &EffectContext) -> ObserveContext {
    cx.operation().observe().expect("observe context")
}

fn next() -> Invocation {
    Invocation::validated(vec!["recover".into()], |args| args == ["recover"]).expect("recovery")
}

fn checkout() -> CheckoutId {
    CheckoutId::new(CheckoutKind::EphemeralClone, [9; 16])
}

fn home() -> effigy_test_support::Fixture {
    effigy_test_support::Fixture::short_home("effigy-session-").expect("fixture home")
}

fn run_id(byte: u8) -> RunId {
    RunId::new(1, [byte; 10]).expect("run id")
}

fn environment() -> ResourceId {
    ResourceId::environment(checkout(), "stack").expect("environment")
}

fn workspace_entry() -> ResourceId {
    TaskId::new(checkout(), "", Surface::Published, "workspace", None)
        .expect("workspace entry")
        .resource()
}

fn task(name: &str) -> ResourceId {
    TaskId::new(checkout(), "", Surface::Published, name, None)
        .expect("task")
        .resource()
}

fn inv(owner: ResourceId, name: &str) -> ResolvedInvocation {
    ResolvedInvocation::new(crate::id::InvocationSpec {
        task: TaskId::new(checkout(), "", Surface::Published, name, None).expect("task id"),
        route: Route::Task,
        argv: vec![],
        cwd: "".into(),
        target: ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: vec![],
        attachments: vec![],
        class: EffectClass::Mutate,
        placement: crate::ctx::Placement::Host,
        provenance: BTreeMap::new(),
    })
    .expect("invocation")
}

fn opts(id: RunId) -> BeginOpts {
    BeginOpts {
        id,
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: None,
    }
}

#[derive(Debug, Default)]
struct NoEffects;
impl SettlementAdapter for NoEffects {
    fn observe(&mut self, effect: &Effect, _cx: &CleanupContext) -> Observed<Fence> {
        Observed::Verified(effect.spec.target.clone(), evidence())
    }
    fn release(&mut self, _effect: &Effect, _cx: &CleanupContext) -> Observed<()> {
        Observed::Verified((), evidence())
    }
    fn reconcile(&mut self, _effect: &Effect, _cx: &CleanupContext) -> Observed<Reconciled> {
        Observed::Verified(Reconciled::Released, evidence())
    }
}

fn adapter() -> NoEffects {
    NoEffects
}

/// A fake environment engine: the resource exists, at generation 1, applied.
fn active_environment(cx: &EffectContext, root: &StateRoot) -> (ResourceId, Fence) {
    let environment = environment();
    let mut store = Store::open(cx, root).expect("store");
    let generation = store
        .transaction(cx, |txn| {
            txn.resource(&environment, "environment")?;
            let generation = txn.next_generation(&environment)?;
            txn.conn().execute(
                "UPDATE resources SET state='active',last_applied=?2,updated_at=?3 WHERE name=?1",
                rusqlite::params![
                    environment.name().to_string(),
                    generation.get().to_string(),
                    txn.now()
                ],
            )?;
            Ok::<_, RunError>(generation)
        })
        .expect("active environment");
    (
        environment.clone(),
        Fence {
            resource: environment,
            generation,
        },
    )
}

/// A run that has not settled is live, whatever phase its last effect left it.
fn is_live(run: &Run<'_>) -> bool {
    !matches!(
        run.phase(),
        Ok(Phase::Stopping) | Ok(Phase::Closed) | Err(_)
    )
}

fn read_journal(cx: &EffectContext, root: &StateRoot) -> StoreReader {
    match Store::read(&observe(cx), root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("the journal is not readable: {other:?}"),
    }
}

fn admit_session<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    environment: &Fence,
) -> (Run<'a>, SessionRef) {
    SessionRef::admit(
        cx,
        root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(1)),
        &cleanup(cx),
        &mut adapter(),
        environment,
        Placement::Detached,
    )
    .expect("workspace session")
}

fn stop_requests(cx: &EffectContext, root: &StateRoot, run: RunId) -> usize {
    let reader = read_journal(cx, root);
    reader
        .conn()
        .query_row(
            "SELECT count(*) FROM requests WHERE run_id=?1 AND state='pending'",
            [run.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .expect("request rows") as usize
}

#[test]
fn first_entry_creates_one_detached_session_run() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (session_run, session) = admit_session(&cx, &root, &fence);

    assert_eq!(session_run.fence().resource, workspace_entry());
    assert_eq!(session.environment(), &environment);
    assert_eq!(
        session.session(),
        run::Session {
            run: session.run(),
            generation: session.generation()
        },
        "the session layer and the kernel seam name one session"
    );
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "the environment resolves exactly the session it admitted"
    );
    // One workspace entry, one unresolved run.
    let open = match run::history(&reader, &workspace_entry(), 10) {
        Observed::Verified(runs, _) => runs
            .into_iter()
            .filter(|run| run.phase != Phase::Closed)
            .count(),
        other => panic!("history is not readable: {other:?}"),
    };
    assert_eq!(open, 1);
    assert_eq!(
        SessionRef::of_run(&reader, session.run())
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone())
    );
}

#[test]
fn a_session_must_be_detached() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let error = SessionRef::admit(
        &cx,
        &root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(2)),
        &cleanup(&cx),
        &mut adapter(),
        &fence,
        crate::supervise::Placement::Inline,
    )
    .expect_err("an inline session is refused");
    assert!(matches!(error, RunError::Refused(_)));
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment())
            .verified()
            .cloned()
            .flatten(),
        None,
        "a refused entry leaves no session"
    );
}

#[test]
fn a_second_entry_attaches_to_the_same_session() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);

    // What a second `workspace` invocation does: resolve, then attach.
    let reader = read_journal(&cx, &root);
    let resolved = SessionRef::resolve(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("the session resolves");
    assert_eq!(resolved, session);
    // A terminal's work is a child of that session, not a second session.
    let child = session_run
        .begin_child(
            &inv(task("dev"), "dev"),
            opts(run_id(9)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("child admission");

    let reader = read_journal(&cx, &root);
    match run::summary(&reader, child.id()) {
        Observed::Verified(summary, _) => assert_eq!(summary.parent, Some(session.run())),
        other => panic!("the child run is not readable: {other:?}"),
    }
    let open = match run::history(&reader, &workspace_entry(), 10) {
        Observed::Verified(runs, _) => runs
            .into_iter()
            .filter(|run| run.phase != Phase::Closed)
            .count(),
        other => panic!("history is not readable: {other:?}"),
    };
    assert_eq!(open, 1, "attaching never admits a second session");
}

#[test]
fn every_front_end_reports_one_session_identity() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);

    // A TUI, a text client and a headless entry all hold this identity; the
    // runner's front-end test renders the same label for each of them.
    let handle = SessionHandle::new(session.clone());
    assert_eq!(handle.identity(), &session);
    assert_eq!(
        handle.session(),
        run::Session {
            run: session.run(),
            generation: session.generation()
        }
    );
    assert!(session.label().contains(&session.run().to_string()));
    assert!(session.label().contains(&session.environment().label()));
}

#[test]
fn owner_loss_maps_to_the_kernel_release_and_cause() {
    // An EOF, a disconnect and a signal are all the client going without a
    // detach, so each records the kernel's `Release::Eof`; only the stop cause
    // distinguishes them.
    for loss in [
        OwnerLoss::EndOfInput,
        OwnerLoss::Disconnected,
        OwnerLoss::Signal(15),
        OwnerLoss::Signal(1),
    ] {
        assert_eq!(loss.release(), Release::Eof);
    }
    assert_eq!(OwnerLoss::EndOfInput.cause(), StopCause::OwnerLost);
    assert_eq!(OwnerLoss::Disconnected.cause(), StopCause::OwnerLost);
    assert_eq!(OwnerLoss::Signal(15).cause(), StopCause::Signal(15));
    assert_eq!(OwnerLoss::Signal(1).cause(), StopCause::Signal(1));
    // An acknowledged detach is not a loss: it uses the kernel's other release
    // and stops nothing.
    assert_ne!(OwnerLoss::EndOfInput.release(), Release::Detach);
}

#[test]
fn down_refuses_a_foreign_holder_and_settles_nothing_else() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let mut foreign = Run::begin(
        &cx,
        &root,
        &inv(task("dev:host"), "dev:host"),
        opts(run_id(70)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("foreign run");
    Attachments::attach(
        &mut foreign,
        &fence,
        AttachMode::Exclusive {
            stop_on_release: false,
        },
        Wait::FailFast,
    )
    .expect("foreign attachment");

    let reader = read_journal(&cx, &root);
    let start = SessionHandle::new(session.clone())
        .down(&observe(&cx), &root, &reader)
        .verified()
        .cloned()
        .expect("down observation");
    match start {
        DownStart::Refused { holders } => {
            assert_eq!(holders, vec![DownHolder::Live(foreign.id())]);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    // Nothing was stopped: no request for the foreign run and it is still live.
    assert!(foreign.requests().expect("requests").is_empty());
    assert!(is_live(&foreign));
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "the session is untouched"
    );
}

#[test]
fn down_refuses_an_unknown_holder() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let mut foreign = Run::begin(
        &cx,
        &root,
        &inv(task("dev:host"), "dev:host"),
        opts(run_id(71)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("foreign run");
    Attachments::attach(
        &mut foreign,
        &fence,
        AttachMode::Exclusive {
            stop_on_release: false,
        },
        Wait::FailFast,
    )
    .expect("foreign attachment");
    let foreign_id = foreign.id();
    // A run whose custodian died still protects the environment (R3/R4).
    drop(foreign);

    let reader = read_journal(&cx, &root);
    let start = SessionHandle::new(session.clone())
        .down(&observe(&cx), &root, &reader)
        .verified()
        .cloned()
        .expect("down observation");
    match start {
        DownStart::Refused { holders } => {
            assert_eq!(holders, vec![DownHolder::Interrupted(foreign_id)]);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn down_stops_children_then_takes_the_permit() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let mut child = session_run
        .begin_child(
            &inv(task("dev"), "dev"),
            opts(run_id(81)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("child admission");

    let reader = read_journal(&cx, &root);
    let children = match SessionHandle::new(session.clone())
        .down(&observe(&cx), &root, &reader)
        .verified()
        .cloned()
        .expect("down observation")
    {
        DownStart::Ready(children) => children,
        other => panic!("expected readiness, got {other:?}"),
    };
    assert_eq!(children.children(), &[child.id()]);

    // A child that has not settled refuses the down.
    match children.unresolved(&reader) {
        Observed::Verified(unresolved, _) => assert_eq!(unresolved, vec![child.id()]),
        other => panic!("children are not readable: {other:?}"),
    }

    let stop = run::RequestId::new([91; 16]);
    children
        .stop(&cx, &root, &reader, &stop, child.id())
        .expect("child stop");
    assert_eq!(stop_requests(&cx, &root, child.id()), 1);
    // A retried stop is the same durable request, not a second one.
    children
        .stop(&cx, &root, &reader, &stop, child.id())
        .expect("the repeated stop reads its recorded request");
    assert_eq!(stop_requests(&cx, &root, child.id()), 1);
    assert!(child
        .requests()
        .expect("requests")
        .iter()
        .any(|request| matches!(request.action, RequestAction::Stop)));

    child
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt");
    child
        .settle(&cleanup(&cx), &mut adapter())
        .expect("settle child");

    let reader = read_journal(&cx, &root);
    match children.unresolved(&reader) {
        Observed::Verified(unresolved, _) => assert!(unresolved.is_empty()),
        other => panic!("children are not readable: {other:?}"),
    }

    // The session must be settling itself before the down's permit is issued.
    let mut down_run = Run::begin(
        &cx,
        &root,
        &inv(environment.clone(), "environment:down"),
        opts(run_id(82)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("down run");
    assert!(
        children.permit(&mut down_run).is_err(),
        "a live session holder refuses the down permit"
    );

    session_run
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt session");
    let mut permit = children
        .permit(&mut down_run)
        .expect("the only holder is the exempt session");
    permit
        .intend(EffectSpec {
            target: permit.fence().clone(),
            kind: "environment:stopped".into(),
            payload: serde_json::Value::Null,
            settlement: Settlement::Release(serde_json::Value::Null),
            required: false,
        })
        .expect("down intent");
}

/// A run that borrows the environment, the way `hold()` attaches a persistent
/// environment's holder. A child or a host `dev` hold does this too, so the
/// attachment alone never says which run is the session.
fn hold_borrowing(holder: &mut Run<'_>, environment: &Fence) {
    Attachments::attach(holder, environment, AttachMode::Borrow, Wait::FailFast)
        .expect("borrowing attachment");
}

#[test]
fn a_borrowing_child_run_never_masks_the_session() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);

    // The reviewer's probe: a child of the session that borrows the same
    // environment, as a persistent `hold()` does.
    let mut child = session_run
        .begin_child(
            &inv(task("dev"), "dev"),
            opts(run_id(31)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("child admission");
    hold_borrowing(&mut child, &fence);

    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "a borrowing child is the session's work, never the session"
    );
    // And the down still names it as the session's own child, not a foreign
    // holder, so the session is not refused against itself.
    let children = match SessionHandle::new(session.clone())
        .down(&observe(&cx), &root, &reader)
        .verified()
        .cloned()
        .expect("down observation")
    {
        DownStart::Ready(children) => children,
        other => panic!("expected readiness, got {other:?}"),
    };
    assert_eq!(children.children(), &[child.id()]);
}

#[test]
fn a_borrowing_root_run_is_not_the_session() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);

    // A host `dev` run on a persistent environment: root (no parent), Task
    // owned, borrowing. Only the recorded mapping can tell it apart.
    let mut host = Run::begin(
        &cx,
        &root,
        &inv(task("dev:host"), "dev:host"),
        opts(run_id(32)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("host run");
    hold_borrowing(&mut host, &fence);

    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "a borrowing root run is not the session"
    );
    // It holds the environment, so a down refuses and names it.
    match SessionHandle::new(session.clone())
        .down(&observe(&cx), &root, &reader)
        .verified()
        .cloned()
        .expect("down observation")
    {
        DownStart::Refused { holders } => {
            assert_eq!(holders, vec![DownHolder::Live(host.id())]);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    assert!(is_live(&host), "the foreign holder is untouched");
}

#[test]
fn a_superseded_generation_keeps_the_predecessor_until_a_successor_commits() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);

    // A newer environment generation no longer revokes the recorded session by
    // itself: the predecessor stays authoritative until a successor activation
    // commits (owner ruling, 2026-10-29).
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let mut store = Store::open(&cx, &root).expect("store");
    store
        .transaction(&cx, |txn| {
            let next = txn.next_generation(&environment)?;
            txn.conn().execute(
                "UPDATE resources SET state='active',last_applied=?2,updated_at=?3 WHERE name=?1",
                rusqlite::params![
                    environment.name().to_string(),
                    next.get().to_string(),
                    txn.now()
                ],
            )?;
            Ok::<_, RunError>(())
        })
        .expect("environment generation advanced");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "the predecessor is authoritative until a successor commits"
    );

    // A settled session does not resolve.
    session_run
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt");
    session_run
        .settle(&cleanup(&cx), &mut adapter())
        .expect("settle");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        None
    );
}

// ---------------------------------------------------------------------------
// Workspace activation reservation (§8.7, §8.11)
// ---------------------------------------------------------------------------
//
// The circle these tests prove is broken: E is known before the Task-owned
// session is admitted, `Gate(Env)` is never held while `Gate(Task)` is waited
// for, and the mapping, `active(E)` and the session's attachment appear in one
// transaction or not at all. The successor identity is *staged* while the
// predecessor may still be live, and the predecessor stays authoritative until
// the successor's atomic commit.

use rusqlite::OptionalExtension;

/// An environment that exists and was never activated: generation 0, so its
/// first reserved successor generation is 1.
fn pending_environment(cx: &EffectContext, root: &StateRoot) -> ResourceId {
    let environment = environment();
    let mut store = Store::open(cx, root).expect("store");
    store
        .transaction(cx, |txn| {
            txn.resource(&environment, "environment")?;
            Ok::<_, RunError>(())
        })
        .expect("environment row");
    environment
}

/// A well-formed guest publication receipt for `fence`.
fn receipt(fence: &Fence) -> run::GuestPublicationReceipt {
    run::GuestPublicationReceipt {
        environment: fence.clone(),
        container_id: "a1".repeat(32),
        vm_boot_id: "boot-0001".to_owned(),
        artifact_digest: format!("sha256:{}", "b2".repeat(32)),
        context_generation: 1,
    }
}

/// Stage the successor identity this activation will publish: the exact
/// workspace owner and the run that will admit it. No run exists yet and no gate
/// on the owner is taken, so a live predecessor session is untouched.
fn stage_successor(
    cx: &EffectContext,
    root: &StateRoot,
    reservation: &WorkspaceActivationReservation,
    run: RunId,
) -> SessionBinding {
    reservation
        .stage_session_binding(cx, root, &workspace_entry(), run)
        .expect("staged successor")
}

/// The successor session admitted at the frozen identity.
fn admit_frozen<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    reservation: &WorkspaceActivationReservation,
    run: RunId,
) -> (Run<'a>, SessionRef) {
    SessionRef::admit_reserved(
        cx,
        root,
        &inv(workspace_entry(), "workspace"),
        opts(run),
        &cleanup(cx),
        &mut adapter(),
        reservation,
        Placement::Detached,
    )
    .expect("reserved workspace session")
}

/// Stage and admit in one step, for tests that are not about the boundary.
fn admit_staged<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    reservation: &WorkspaceActivationReservation,
    run: RunId,
) -> (Run<'a>, SessionRef) {
    stage_successor(cx, root, reservation, run);
    admit_frozen(cx, root, reservation, run)
}

fn publication_for(
    cx: &EffectContext,
    root: &StateRoot,
    reservation: &WorkspaceActivationReservation,
    run: &Run<'_>,
) -> WorkspacePublicationReceipt {
    let plan = plan_of(reservation, &read_journal(cx, root)).expect("admitted staged plan");
    WorkspacePublicationReceipt {
        publication: receipt(&plan.fence()),
        activation_run: run.id(),
        session: plan.binding().clone(),
    }
}

/// The environment activation run at the generation the reservation names, with
/// its required guest publication applied — exactly the state a commit
/// requires, and nothing more.
fn admit_activation<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    reservation: &WorkspaceActivationReservation,
    id: RunId,
) -> Run<'a> {
    let claim = reservation.claim().expect("claim");
    let mut run = Run::begin_claimed(
        cx,
        root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(id),
        || Ok(cleanup(cx)),
        &mut adapter(),
        claim,
    )
    .expect("activation run");
    let ticket = Attachments::intend_guest_publication(&mut run).expect("publication intent");
    let receipt = publication_for(cx, root, reservation, &run);
    Attachments::workspace_guest_published(&mut run, &ticket, receipt)
        .expect("publication receipt");
    run
}

/// One complete activation: reserve, freeze and admit the successor, admit the
/// environment run at E, publish and commit.
fn activate<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    environment: &ResourceId,
    session_id: RunId,
    activation_id: RunId,
) -> (WorkspaceActivationReservation, Run<'a>, SessionRef, Run<'a>) {
    let reservation =
        WorkspaceActivationReservation::reserve(cx, root, environment, Wait::FailFast)
            .expect("reservation");
    let (session_run, session) = admit_staged(cx, root, &reservation, session_id);
    let environment_run = admit_activation(cx, root, &reservation, activation_id);
    reservation
        .commit(cx, root, &environment_run)
        .expect("activation commit");
    (reservation, session_run, session, environment_run)
}

/// End a run the way its own supervisor does: stop it, then settle it.
fn end_run(cx: &EffectContext, mut run: Run<'_>, outcome: WorkloadOutcome) {
    run.interrupt(&cleanup(cx), outcome).expect("interrupt");
    run.settle(&cleanup(cx), &mut adapter()).expect("settle");
}

fn mapping(reader: &StoreReader, environment: &ResourceId) -> Option<(String, i64, i64)> {
    reader
        .conn()
        .query_row(
            "SELECT run_id, session_generation, environment_generation FROM workspace_sessions \
             WHERE environment=?1",
            [environment.name().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .expect("mapping row")
}

fn attachment_row(
    reader: &StoreReader,
    environment: &ResourceId,
    run: RunId,
) -> Option<(i64, String, String)> {
    reader
        .conn()
        .query_row(
            "SELECT env_generation, mode, state FROM attachments \
             WHERE environment=?1 AND run_id=?2",
            rusqlite::params![environment.name().to_string(), run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .expect("attachment row")
}

fn resource_row(
    reader: &StoreReader,
    environment: &ResourceId,
) -> (i64, Option<String>, Option<String>) {
    reader
        .conn()
        .query_row(
            "SELECT generation, state, last_applied FROM resources WHERE name=?1",
            [environment.name().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("resource row")
}

fn attachment_effects(reader: &StoreReader, run: RunId) -> i64 {
    reader
        .conn()
        .query_row(
            "SELECT count(*) FROM events WHERE run_id=?1 AND kind=?2",
            rusqlite::params![run.to_string(), "kernel:attachment"],
            |row| row.get(0),
        )
        .expect("attachment effects")
}

fn resolved(reader: &StoreReader, environment: &ResourceId) -> Option<SessionRef> {
    SessionRef::resolve(reader, environment)
        .verified()
        .cloned()
        .flatten()
}

fn plan_of(
    reservation: &WorkspaceActivationReservation,
    reader: &StoreReader,
) -> Option<ActivationPlan> {
    reservation.plan(reader).verified().cloned().flatten()
}

#[test]
fn the_first_activation_commits_mapping_active_and_borrow_together() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    // A predecessor generation, so E (the reserved successor) and S (the
    // session run's own generation) are visibly different fences.
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    assert_eq!(
        reservation.generation(),
        Generation::new(2),
        "E is the next generation"
    );

    // Staging freezes the successor identity and admits nothing.
    let binding = stage_successor(&cx, &root, &reservation, run_id(10));
    assert_eq!(binding.owner, workspace_entry());
    assert_eq!(
        binding.generation,
        Generation::new(1),
        "S is the workspace owner's next generation, never E"
    );
    assert_eq!(binding.run, run_id(10));
    let reader = read_journal(&cx, &root);
    assert_eq!(
        plan_of(&reservation, &reader),
        None,
        "a staged identity with no admitted run is not ready to publish"
    );
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(
        attachment_row(&reader, &environment, run_id(10)),
        None,
        "staging attaches nothing"
    );

    // The frozen identity is the one that admits.
    let (session_run, session) = admit_frozen(&cx, &root, &reservation, run_id(10));
    assert_eq!(
        session.generation(),
        Generation::new(1),
        "S is the run's own fence"
    );
    assert_eq!(
        session.environment_generation(),
        Generation::new(2),
        "E labels the session"
    );
    assert_ne!(session.generation(), session.environment_generation());
    let reader = read_journal(&cx, &root);
    let plan = plan_of(&reservation, &reader).expect("ready plan");
    assert_eq!(plan.session_owner(), &workspace_entry());
    assert_eq!(plan.session_run(), session_run.id());
    assert_eq!(plan.session_generation(), session.generation());
    assert_eq!(plan.environment_generation(), Generation::new(2));
    assert_eq!(
        resolved(&reader, &environment),
        None,
        "an uncommitted session does not resolve"
    );

    let environment_run = admit_activation(&cx, &root, &reservation, run_id(11));
    assert_eq!(environment_run.fence().generation, Generation::new(2));
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("commit");
    assert_eq!(committed.session(), session);

    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(session.clone()),
        "the committed session resolves as the environment's session"
    );
    assert_eq!(
        mapping(&reader, &environment),
        Some((
            session_run.id().to_string(),
            session.generation().get() as i64,
            session.environment_generation().get() as i64
        )),
        "the mapping carries S and E separately"
    );
    assert_eq!(
        attachment_row(&reader, &environment, session_run.id()),
        Some((2, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert_eq!(attachment_effects(&reader, session_run.id()), 1);
    let (generation, state, applied) = resource_row(&reader, &environment);
    assert_eq!(generation, 2);
    assert_eq!(state.as_deref(), Some("active"));
    assert_eq!(applied.as_deref(), Some("2"));
    assert_eq!(
        WorkspaceActivationReservation::recover(&reader, &environment)
            .verified()
            .cloned()
            .flatten()
            .map(|recovered| recovered.state()),
        Some(ActivationReservationState::Committed)
    );
}

#[test]
fn reserve_recovers_the_same_reservation_and_generation() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    // A first-ever activation reserves generation 1: the reservation writes the
    // environment's record, so nothing has to exist before it.
    let environment = environment();
    let first = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("first reservation");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        resource_row(&reader, &environment).0,
        0,
        "the row is reserved, not activated"
    );
    assert_eq!(first.generation(), Generation::new(1));
    // A competing reservation for the same next generation is the same
    // reservation: one environment has one successor generation.
    let second = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("a competing reservation recovers");
    assert_eq!(first.id(), second.id());
    assert_eq!(first.generation(), second.generation());
    let reader = read_journal(&cx, &root);
    assert_eq!(
        WorkspaceActivationReservation::recover(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(first),
        "a caller that lost its handle recovers the same reservation"
    );
    let store_count: i64 = reader
        .conn()
        .query_row(
            "SELECT count(*) FROM workspace_activation_reservations WHERE environment=?1",
            [environment.name().to_string()],
            |row| row.get(0),
        )
        .expect("reservation rows");
    assert_eq!(store_count, 1);
}

#[test]
fn a_competing_environment_activation_is_fenced_by_name() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    // An activation that does not name the reservation cannot take E.
    let error = Run::begin(
        &cx,
        &root,
        &inv(environment.clone(), "environment:down"),
        opts(run_id(20)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect_err("a reserved generation is not admitted around its reservation");
    match &error {
        RunError::Refused(text) => assert!(
            text.contains("reserved by a workspace activation"),
            "the refusal names the reservation: {text}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    // Cancelling the reservation releases the fence, so the refusal is a fence
    // and not a second authority.
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("cancel");
    let plain = Run::begin(
        &cx,
        &root,
        &inv(environment.clone(), "environment:down"),
        opts(run_id(21)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("an unreserved generation is admitted as before");
    drop(plain);
}

#[test]
fn staging_freezes_one_successor_identity_for_the_workspace_owner() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let frozen = stage_successor(&cx, &root, &reservation, run_id(30));
    // Staging the same identity again is the same staging, not a second one.
    assert_eq!(
        stage_successor(&cx, &root, &reservation, run_id(30)),
        frozen
    );
    // The first staging names the reservation's workspace owner: another Task
    // run can neither replace the owner nor the run.
    let other = task("workspace-other");
    let error = reservation
        .stage_session_binding(&cx, &root, &other, run_id(30))
        .expect_err("a staged reservation names one workspace owner");
    assert!(matches!(error, RunError::Refused(_)));
    let error = reservation
        .stage_session_binding(&cx, &root, &other, run_id(32))
        .expect_err("a competing owner never stages this reservation");
    assert!(matches!(error, RunError::Refused(_)));
    assert!(
        reservation
            .stage_session_binding(&cx, &root, &workspace_entry(), run_id(31))
            .is_err(),
        "another unadmitted contender cannot replace the winner"
    );
    // The frozen run admits, and a competing Task run cannot then replace it.
    let (session_run, _session) = admit_frozen(&cx, &root, &reservation, run_id(30));
    assert!(is_live(&session_run));
    let error = reservation
        .stage_session_binding(&cx, &root, &workspace_entry(), run_id(31))
        .expect_err("an admitted successor is never replaced");
    match &error {
        RunError::Refused(text) => assert!(
            text.contains("live staged workspace session"),
            "the refusal names the staged session: {text}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let reader = read_journal(&cx, &root);
    assert_eq!(
        plan_of(&reservation, &reader).map(|plan| plan.session_run()),
        Some(session_run.id()),
        "the frozen identity is unchanged"
    );
}

#[test]
fn a_staged_successor_admits_only_the_frozen_run() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let frozen = stage_successor(&cx, &root, &reservation, run_id(40));
    // A competing Task run cannot admit itself against the frozen staging.
    let competing = SessionRef::admit_reserved(
        &cx,
        &root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(41)),
        &cleanup(&cx),
        &mut adapter(),
        &reservation,
        Placement::Detached,
    );
    match competing {
        Err(RunError::Refused(text)) => assert!(
            text.contains("not the successor session the reservation froze"),
            "the refusal names the frozen identity: {text}"
        ),
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("a run the staging never froze must not be admitted"),
    }
    let reader = read_journal(&cx, &root);
    assert!(
        run::summary(&reader, run_id(41)).verified().is_none(),
        "a refused admission writes no run"
    );
    // The frozen run admits, at exactly the staged generation.
    let (run, session) = admit_frozen(&cx, &root, &reservation, frozen.run);
    assert_eq!(session.generation(), frozen.generation);
    assert_eq!(session.generation(), run.fence().generation);
}

#[test]
fn a_frozen_staging_is_never_replaced_after_publication() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let binding = stage_successor(&cx, &root, &reservation, run_id(50));
    let (_session_run, _session) = admit_frozen(&cx, &root, &reservation, binding.run);
    // The activation run claims E and publishes the guest pointer.
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(51));
    let reader = read_journal(&cx, &root);
    assert_eq!(
        plan_of(&reservation, &reader).map(|plan| plan.session_run()),
        Some(binding.run),
        "the plan is ready and frozen"
    );
    // A replacement after publication is refused: the frozen identity can never
    // change under a receipt that was already published.
    let error = reservation
        .stage_session_binding(&cx, &root, &workspace_entry(), run_id(52))
        .expect_err("a published activation's session is frozen");
    match &error {
        RunError::Refused(text) => assert!(
            text.contains("staged workspace session is frozen"),
            "the refusal names the freeze: {text}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    // The frozen identity still commits.
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("the frozen activation commits");
    assert_eq!(committed.session().run(), binding.run);
}

#[test]
fn a_receipt_is_validated_against_the_frozen_plan_before_the_commit() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (session_run, session) = admit_staged(&cx, &root, &reservation, run_id(53));
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(54));
    let reader = read_journal(&cx, &root);
    let plan = plan_of(&reservation, &reader).expect("ready plan");
    assert_eq!(plan.session_run(), session_run.id());
    assert_eq!(plan.session_generation(), session.generation());

    // The reversible failing safety assertion: a receipt naming the session's
    // own generation S is refused, and the same receipt naming E is accepted.
    let mut wrong = publication_for(&cx, &root, &reservation, &environment_run);
    wrong.publication.environment.generation = plan.session_generation();
    assert_ne!(plan.session_generation(), plan.environment_generation());
    assert!(
        plan.validate_receipt(&wrong).is_err(),
        "a receipt for S must not validate a commit at E"
    );
    assert!(plan
        .validate_receipt(&publication_for(&cx, &root, &reservation, &environment_run))
        .is_ok());
    for field in ["owner", "run", "S", "activation"] {
        let mut wrong = publication_for(&cx, &root, &reservation, &environment_run);
        match field {
            "owner" => wrong.session.owner = task("another-workspace"),
            "run" => wrong.session.run = run_id(200),
            "S" => wrong.session.generation = Generation::new(200),
            "activation" => wrong.activation_run = run_id(201),
            _ => unreachable!(),
        }
        assert!(
            plan.validate_receipt(&wrong).is_err(),
            "receipt must authenticate {field}"
        );
    }
    // And a frozen plan is what the commit validates the recorded receipt with.
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("the activation commits at E");
    assert_eq!(committed.session(), session);
}

#[test]
fn a_commit_without_an_applied_publication_writes_nothing() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (session_run, session) = admit_staged(&cx, &root, &reservation, run_id(55));
    let claim = reservation.claim().expect("claim");
    let mut environment_run = Run::begin_claimed(
        &cx,
        &root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(56)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        claim,
    )
    .expect("activation run");
    // The intent alone is not a receipt: no active publication may precede it.
    let ticket =
        Attachments::intend_guest_publication(&mut environment_run).expect("publication intent");
    let error = reservation
        .commit(&cx, &root, &environment_run)
        .expect_err("an intended publication is not a receipt");
    assert!(matches!(error, RunError::Refused(_)));
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(
        attachment_row(&reader, &environment, session_run.id()),
        None
    );
    assert_eq!(attachment_effects(&reader, session_run.id()), 0);
    assert_eq!(
        resource_row(&reader, &environment).1.as_deref(),
        Some("pending")
    );
    assert_eq!(
        WorkspaceActivationReservation::recover(&reader, &environment)
            .verified()
            .cloned()
            .flatten()
            .map(|recovered| recovered.state()),
        Some(ActivationReservationState::Reserved),
        "a refused commit changes no reservation state"
    );
    for field in ["owner", "run", "S", "activation"] {
        let mut wrong = publication_for(&cx, &root, &reservation, &environment_run);
        match field {
            "owner" => wrong.session.owner = task("wrong-owner"),
            "run" => wrong.session.run = run_id(202),
            "S" => wrong.session.generation = Generation::new(202),
            "activation" => wrong.activation_run = run_id(203),
            _ => unreachable!(),
        }
        assert!(
            Attachments::workspace_guest_published(&mut environment_run, &ticket, wrong).is_err(),
            "recording refuses a receipt with the wrong {field}"
        );
    }
    // The same run can still commit once its publication is complete.
    let receipt = publication_for(&cx, &root, &reservation, &environment_run);
    Attachments::workspace_guest_published(&mut environment_run, &ticket, receipt)
        .expect("publication receipt");
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("commit after the receipt");
    assert_eq!(committed.session(), session);
}

#[test]
fn a_commit_refuses_an_activation_run_that_cannot_publish() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");

    // A required effect that never completed refuses the commit.
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (_session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(57));
    let claim = reservation.claim().expect("claim");
    let mut environment_run = Run::begin_claimed(
        &cx,
        &root,
        &inv(environment.clone(), "environment:up"),
        opts(run_id(58)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        claim,
    )
    .expect("activation run");
    let first = Attachments::intend_guest_publication(&mut environment_run)
        .expect("first publication intent");
    let _second = Attachments::intend_guest_publication(&mut environment_run)
        .expect("a second required publication");
    let receipt = publication_for(&cx, &root, &reservation, &environment_run);
    Attachments::workspace_guest_published(&mut environment_run, &first, receipt)
        .expect("publication receipt");
    let error = reservation
        .commit(&cx, &root, &environment_run)
        .expect_err("a required effect is still unresolved");
    match &error {
        RunError::Refused(text) => assert!(
            text.contains("required effect"),
            "the refusal names the unresolved obligation: {text}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(attachment_effects(&reader, run_id(57)), 0);

    // A run that is settling may not publish either.
    let mut environment_run = environment_run;
    environment_run
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt the activation");
    let error = reservation
        .commit(&cx, &root, &environment_run)
        .expect_err("a settling run publishes nothing");
    assert!(matches!(error, RunError::Refused(_)));
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
}

#[test]
fn a_commit_is_idempotent_for_the_run_that_made_it() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (reservation, _session_run, session, environment_run) =
        activate(&cx, &root, &environment, run_id(60), run_id(61));
    let reader = read_journal(&cx, &root);
    let recovered = WorkspaceActivationReservation::recover(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("the committed reservation");
    let again = recovered
        .commit(&cx, &root, &environment_run)
        .expect("a retried commit answers with the same session");
    assert_eq!(again.session(), session);
    assert_eq!(
        attachment_effects(&reader, session.run()),
        1,
        "no second attachment"
    );
    assert_eq!(reservation.state(), ActivationReservationState::Reserved);
}

#[test]
fn an_unknown_stored_state_is_refused() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("reservation");
    let mut store = Store::open(&cx, &root).expect("store");
    store
        .transaction(&cx, |txn| {
            txn.conn().execute(
                "UPDATE workspace_activation_reservations SET state='quiescing' WHERE environment=?1",
                [environment.name().to_string()],
            )?;
            Ok::<_, RunError>(())
        })
        .expect("a state this kernel does not know");
    let reader = read_journal(&cx, &root);
    assert!(
        matches!(
            WorkspaceActivationReservation::recover(&reader, &environment),
            Observed::Unknown(_)
        ),
        "a state this kernel does not know is Unknown, never guessed"
    );
    let error = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect_err("reserve refuses an unreadable reservation record");
    assert!(matches!(error, RunError::Broken(_)));

    // Half a stored binding is not a binding either.
    store
        .transaction(&cx, |txn| {
            txn.conn().execute(
                "UPDATE workspace_activation_reservations SET state='reserved',session_run=?2 \
                 WHERE environment=?1",
                rusqlite::params![environment.name().to_string(), run_id(62).to_string()],
            )?;
            Ok::<_, RunError>(())
        })
        .expect("half a binding");
    let reader = read_journal(&cx, &root);
    assert!(matches!(
        WorkspaceActivationReservation::recover(&reader, &environment),
        Observed::Unknown(_)
    ));
}

#[test]
fn cancel_and_revive_keep_the_generation_and_never_reuse_a_consumed_one() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let cancelled = reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("cancel before any run");
    assert_eq!(cancelled.state(), ActivationReservationState::Cancelled);
    // The generation was never taken, so the same reservation revives.
    let revived = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("revive");
    assert_eq!(revived.generation(), reservation.generation());
    assert_eq!(revived.id(), reservation.id());
    assert_eq!(revived.state(), ActivationReservationState::Reserved);

    // A claimed generation belongs to its run until that run settles.
    let (claimed, _session_run, _session, environment_run) =
        activate(&cx, &root, &environment, run_id(63), run_id(64));
    drop(environment_run);
    let error = claimed
        .cancel(&cx, &root, Wait::FailFast)
        .expect_err("an interrupted run row still owns its admission");
    assert!(matches!(error, RunError::Refused(_)));
}

#[test]
fn a_crash_after_reserve_before_commit_leaves_the_predecessor_authoritative() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);

    // Generation 1: a committed, live predecessor session serving the
    // environment. The activation run that published it settles normally.
    let (_first, predecessor, first, first_activation) =
        activate(&cx, &root, &environment, run_id(70), run_id(71));
    end_run(
        &cx,
        first_activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let authoritative = |reader: &StoreReader| {
        assert_eq!(
            resolved(reader, &environment),
            Some(first.clone()),
            "the predecessor stays the environment's session"
        );
        assert_eq!(
            attachment_row(reader, &environment, first.run()),
            Some((1, "\"Borrow\"".to_owned(), "attached".to_owned())),
            "the predecessor keeps its own attachment and generation"
        );
        assert_eq!(
            mapping(reader, &environment).map(|row| row.0),
            Some(first.run().to_string())
        );
    };
    let reader = read_journal(&cx, &root);
    authoritative(&reader);

    // Boundary: after reserve, before staging.
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    assert_eq!(reservation.generation(), Generation::new(2));
    let reader = read_journal(&cx, &root);
    authoritative(&reader);
    assert_eq!(plan_of(&reservation, &reader), None, "nothing is ready");

    // Boundary: after staging, before the successor is admitted.
    let frozen = stage_successor(&cx, &root, &reservation, run_id(72));
    assert_eq!(frozen.generation, Generation::new(2));
    let reader = read_journal(&cx, &root);
    authoritative(&reader);
    assert_eq!(plan_of(&reservation, &reader), None, "still not ready");
    let (staged_run, _staged_session) = admit_frozen(&cx, &root, &reservation, run_id(72));
    assert!(is_live(&predecessor));
    let reader = read_journal(&cx, &root);
    authoritative(&reader);
    assert_eq!(
        stop_requests(&cx, &root, first.run()),
        0,
        "no stop was requested of the predecessor"
    );

    // Boundary: the guest pointer is published, before the commit. The
    // activation run advances the environment to E; the predecessor's record is
    // still authoritative.
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(73));
    assert_eq!(resource_row(&reader, &environment).0, 2);
    let reader = read_journal(&cx, &root);
    let (_, state, _) = resource_row(&reader, &environment);
    assert_eq!(
        state.as_deref(),
        Some("pending"),
        "an uncommitted publication advertises nothing"
    );
    authoritative(&reader);
    // Crash here: the run is gone and nothing was committed.
    drop(environment_run);
    let reader = read_journal(&cx, &root);
    authoritative(&reader);
    assert_eq!(mapping(&reader, &environment).map(|row| row.2), Some(1));

    // Direct reserve-after-crash must recover E, not hide it behind E+1.
    let retry = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("direct retry recovers the unfinished fence");
    assert_eq!(retry.id(), reservation.id());
    assert_eq!(retry.record().environment_run, Some(run_id(73)));
    // A journal from the earlier implementation may already contain a later
    // empty fence. Both entry points still expose the older bound reservation.
    Store::open(&cx, &root)
        .expect("store")
        .transaction(&cx, |txn| {
            WorkspaceActivationReservation::insert(txn, &environment, Generation::new(3))
        })
        .expect("legacy later empty reservation");
    assert_eq!(
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("older pending fence remains visible")
            .id(),
        reservation.id()
    );
    // The reservation is recoverable and releasable: its interrupted activation
    // run settles, then its fence is cancelled without touching the predecessor.
    let recovered = WorkspaceActivationReservation::recover(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("the reservation survives the crash");
    assert_eq!(recovered.state(), ActivationReservationState::Reserved);
    assert_eq!(recovered.record().environment_run, Some(run_id(73)));
    run::settle_interrupted(
        &cx,
        &root,
        &environment,
        run_id(73),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("the crashed activation run settles");
    drop(staged_run);
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(72),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("recover successor without stopping predecessor");
    let cancelled = recovered
        .cancel(&cx, &root, Wait::FailFast)
        .expect("the reservation is releasable");
    assert_eq!(cancelled.state(), ActivationReservationState::Cancelled);
    let reader = read_journal(&cx, &root);
    authoritative(&reader);

    // Roll forward: the next generation stages a new successor, the predecessor
    // ends on its own terms, and the successor's atomic commit replaces the
    // mapping.
    let next_reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("the next generation");
    assert_eq!(next_reservation.generation(), Generation::new(3));
    let next_binding = stage_successor(&cx, &root, &next_reservation, run_id(74));
    let (successor_run, successor) = admit_frozen(&cx, &root, &next_reservation, next_binding.run);
    let environment_run = admit_activation(&cx, &root, &next_reservation, run_id(75));
    let committed = next_reservation
        .commit(&cx, &root, &environment_run)
        .expect("the successor commits");
    assert_eq!(committed.session(), successor);
    let reader = read_journal(&cx, &root);
    assert_eq!(resolved(&reader, &environment), Some(successor.clone()));
    assert_eq!(successor.environment_generation(), Generation::new(3));
    assert_eq!(
        attachment_row(&reader, &environment, successor_run.id()),
        Some((3, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert_eq!(stop_requests(&cx, &root, first.run()), 0, "still no stop");
}

#[test]
fn a_successor_preserves_a_live_holder_and_the_predecessor_session_contract() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);

    // Generation 1 activates with its own session.
    let (_reservation, predecessor, first, first_activation) =
        activate(&cx, &root, &environment, run_id(80), run_id(81));
    end_run(
        &cx,
        first_activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let reader = read_journal(&cx, &root);
    assert_eq!(resolved(&reader, &environment), Some(first.clone()));
    // A live holder of the predecessor generation: a host `dev` run that
    // borrows the environment. A successor must leave it exactly as it was.
    let mut host = Run::begin(
        &cx,
        &root,
        &inv(task("dev:host"), "dev:host"),
        opts(run_id(82)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("host run");
    Attachments::attach(
        &mut host,
        &Fence {
            resource: environment.clone(),
            generation: Generation::new(1),
        },
        AttachMode::Borrow,
        Wait::FailFast,
    )
    .expect("host attachment");

    // The successor is staged while the predecessor session is live: nothing
    // about the predecessor or the holder changes.
    let successor_reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("successor reservation");
    assert_eq!(successor_reservation.generation(), Generation::new(2));
    let binding = stage_successor(&cx, &root, &successor_reservation, run_id(83));
    let reader = read_journal(&cx, &root);
    assert_eq!(resolved(&reader, &environment), Some(first.clone()));
    assert_eq!(
        attachment_row(&reader, &environment, first.run()),
        Some((1, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert!(is_live(&predecessor));

    // The admitted successor commits while the predecessor remains live.
    let (successor_session_run, successor_session) =
        admit_frozen(&cx, &root, &successor_reservation, binding.run);
    let environment_run = admit_activation(&cx, &root, &successor_reservation, run_id(84));
    successor_reservation
        .commit(&cx, &root, &environment_run)
        .expect("successor commit");

    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(successor_session.clone())
    );
    assert_eq!(
        successor_session.environment_generation(),
        Generation::new(2)
    );
    assert_eq!(first.environment_generation(), Generation::new(1));
    // The live holder is untouched: same attachment generation, still attached,
    // still live, and its bytes and listener were never rebound.
    assert_eq!(
        attachment_row(&reader, &environment, host.id()),
        Some((1, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert!(is_live(&host));
    assert_eq!(stop_requests(&cx, &root, host.id()), 0);
    assert_eq!(
        attachment_row(&reader, &environment, successor_session_run.id()),
        Some((2, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert!(is_live(&predecessor));
    assert_eq!(
        attachment_row(&reader, &environment, first.run()),
        Some((1, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert_eq!(stop_requests(&cx, &root, first.run()), 0);
    // Earlier sessions keep run-bound lifecycle authority after the Task fence
    // advanced, and explicit recovery cannot take a living private slot.
    assert!(run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        first.run(),
        next(),
        &cleanup(&cx),
        &mut adapter()
    )
    .is_err());
    end_run(
        &cx,
        predecessor,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

/// Fail each durable crossing, then lose both successor handles as in a crash.
/// The predecessor's selected bytes/listener receipt and attachment must survive;
/// publication/mapping/attachment is all committed or none is committed.
#[cfg(feature = "fault-injection")]
#[test]
fn every_activation_boundary_recovers_without_displacing_the_live_predecessor() {
    use crate::fault::{arm, arm_nth, fired, Fault, Point};
    let boundaries = [
        (Point::ReservationBefore, 0),
        (Point::ReservationAfter, 0),
        (Point::SessionStageBefore, 0),
        (Point::SessionStageAfter, 0),
        (Point::BeginAdmitted, 0),
        (Point::BeginAdmitted, 1),
        (Point::AppliedBefore, 0),
        (Point::AppliedAfter, 0),
        (Point::ActivationCommitBefore, 0),
        (Point::ActivationMappingBefore, 0),
        (Point::ActivationAttachmentBefore, 0),
        (Point::ActivationCommitAfter, 0),
    ];
    for (point, nth) in boundaries {
        let cx = context();
        let fixture = home();
        let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
        let environment = pending_environment(&cx, &root);
        let (_, mut predecessor, first, first_activation) =
            activate(&cx, &root, &environment, run_id(100), run_id(101));
        end_run(
            &cx,
            first_activation,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        );
        let ticket = predecessor.intend(run::EffectSpec {
            target: predecessor.fence().clone(), kind: "fake.selected-session".into(),
            payload: serde_json::json!({"bytes": "immutable-v1", "listener": "original-socket"}),
            settlement: run::Settlement::Retain, required: false,
        }).expect("selected bytes/listener intent");
        predecessor
            .applied(
                &ticket,
                serde_json::json!({"selected": "v1", "listener": "original-socket"}),
            )
            .expect("selected receipt");
        let selected_path = fixture.path().join("selected-bytes");
        std::fs::write(&selected_path, b"immutable-v1").expect("selected bytes");
        let listener_path = fixture.path().join("predecessor.sock");
        let listener =
            std::os::unix::net::UnixListener::bind(&listener_path).expect("predecessor listener");
        listener.set_nonblocking(true).expect("bounded accept");
        use std::os::unix::fs::MetadataExt;
        let listener_inode = std::fs::metadata(&listener_path)
            .expect("listener inode")
            .ino();
        let selected = serde_json::to_value(predecessor.effects().expect("predecessor effects"))
            .expect("snapshot");
        let mut staged_run = None;
        let mut activation_run = None;
        arm_nth(point, nth, Fault::Enospc);
        let result = (|| -> Result<(), RunError> {
            let reservation =
                WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)?;
            reservation.stage_session_binding(&cx, &root, &workspace_entry(), run_id(102))?;
            let (run, _) = SessionRef::admit_reserved(
                &cx,
                &root,
                &inv(workspace_entry(), "workspace"),
                opts(run_id(102)),
                &cleanup(&cx),
                &mut adapter(),
                &reservation,
                Placement::Detached,
            )?;
            staged_run = Some(run);
            let run = Run::begin_claimed(
                &cx,
                &root,
                &inv(environment.clone(), "environment:up"),
                opts(run_id(103)),
                || Ok(cleanup(&cx)),
                &mut adapter(),
                reservation.claim()?,
            )?;
            activation_run = Some(run);
            let run = activation_run.as_mut().expect("activation run");
            let ticket = Attachments::intend_guest_publication(run)?;
            let receipt = publication_for(&cx, &root, &reservation, run);
            Attachments::workspace_guest_published(run, &ticket, receipt)?;
            reservation.commit(&cx, &root, run)?;
            Ok(())
        })();
        assert!(fired(), "fault {point:?}/{nth} must fire");
        assert!(
            result.is_err(),
            "fault {point:?}/{nth} must not report success"
        );
        arm(None);
        drop(activation_run);
        drop(staged_run);
        let reader = read_journal(&cx, &root);
        let committed = point == Point::ActivationCommitAfter;
        assert_eq!(
            mapping(&reader, &environment).map(|r| r.0),
            Some(if committed { run_id(102) } else { first.run() }.to_string()),
            "{point:?}"
        );
        assert_eq!(
            attachment_effects(&reader, run_id(102)),
            if committed { 1 } else { 0 },
            "{point:?}"
        );
        assert_eq!(
            attachment_row(&reader, &environment, first.run()),
            Some((1, "\"Borrow\"".to_owned(), "attached".to_owned())),
            "{point:?}"
        );
        assert_eq!(
            serde_json::to_value(predecessor.effects().expect("effects")).expect("snapshot"),
            selected,
            "fault {point:?} never reselects bytes or listener"
        );
        assert!(
            predecessor.requests().is_ok(),
            "predecessor lifecycle authority survives {point:?}"
        );
        assert_eq!(stop_requests(&cx, &root, first.run()), 0);
        assert!(matches!(
            reader.run_owner_state(&observe(&cx), &root, &first.run()),
            Observed::Verified(OwnerState::Held, _)
        ));
        if !committed {
            assert_eq!(
                resolved(&reader, &environment),
                Some(first.clone()),
                "{point:?}"
            );
            assert_eq!(
                resource_row(&reader, &workspace_entry()).0,
                1,
                "Task fence must not advance before atomic commit at {point:?}"
            );
        } else {
            assert_eq!(resource_row(&reader, &workspace_entry()).0, 2);
            assert_eq!(
                resource_row(&reader, &environment),
                (2, Some("active".into()), Some("2".into()))
            );
        }
        // No replay of guest publication: recover and settle the exact dead
        // runs, then release only the still-reserved fence.
        for (owner, id) in [
            (environment.clone(), run_id(103)),
            (workspace_entry(), run_id(102)),
        ] {
            if run::summary(&reader, id).verified().is_some() {
                run::settle_interrupted(
                    &cx,
                    &root,
                    &owner,
                    id,
                    next(),
                    &cleanup(&cx),
                    &mut adapter(),
                )
                .unwrap_or_else(|e| panic!("recover {id} after {point:?}: {e}"));
            }
        }
        assert_eq!(
            std::fs::read(&selected_path).expect("selected bytes"),
            b"immutable-v1"
        );
        assert_eq!(
            std::fs::metadata(&listener_path)
                .expect("same listener")
                .ino(),
            listener_inode
        );
        let _client = std::os::unix::net::UnixStream::connect(&listener_path)
            .expect("predecessor still accepts");
        let (_accepted, _) = listener
            .accept()
            .expect("same listener accepts after fault and recovery");
        let recovered =
            WorkspaceActivationReservation::recover(&read_journal(&cx, &root), &environment)
                .verified()
                .cloned()
                .flatten()
                .expect("durable reservation");
        if recovered.state() == ActivationReservationState::Reserved {
            recovered
                .cancel(&cx, &root, Wait::FailFast)
                .expect("release failed activation");
        }
        let (_, successor_run, successor, _) =
            activate(&cx, &root, &environment, run_id(104), run_id(105));
        assert_eq!(
            resolved(&read_journal(&cx, &root), &environment),
            Some(successor)
        );
        assert!(predecessor.requests().is_ok());
        assert!(is_live(&successor_run));
    }
}

#[test]
fn three_overlapping_sessions_keep_their_exact_liveness_and_lifecycle_fences() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let mut sessions = Vec::new();
    for (session_id, activation_id) in [(110, 111), (112, 113), (114, 115)] {
        let (_, run, session, activation) = activate(
            &cx,
            &root,
            &environment,
            run_id(session_id),
            run_id(activation_id),
        );
        end_run(
            &cx,
            activation,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        );
        sessions.push((run, session));
        for (run, session) in &sessions {
            assert!(
                run.requests().is_ok(),
                "each admitted session retains its own lifecycle authority"
            );
            assert!(matches!(
                read_journal(&cx, &root).run_owner_state(&observe(&cx), &root, &session.run()),
                Observed::Verified(OwnerState::Held, _)
            ));
            assert!(
                run::settle_interrupted(
                    &cx,
                    &root,
                    &workspace_entry(),
                    session.run(),
                    next(),
                    &cleanup(&cx),
                    &mut adapter()
                )
                .is_err(),
                "recovery cannot settle any live private slot"
            );
        }
        assert!(
            Run::begin(
                &cx,
                &root,
                &inv(workspace_entry(), "workspace"),
                opts(run_id(116)),
                &cleanup(&cx),
                &mut adapter()
            )
            .is_err(),
            "ordinary K5 never admits around private sessions"
        );
    }
    let (oldest, oldest_session) = sessions.remove(0);
    drop(oldest);
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        oldest_session.run(),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("recover oldest private slot after mapping replacement");
    for (run, _) in sessions {
        end_run(
            &cx,
            run,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        );
    }
}

#[test]
fn staged_and_retired_sessions_cannot_create_shared_owner_effects() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let (_, mut predecessor, _, activation) =
        activate(&cx, &root, &environment, run_id(120), run_id(121));
    end_run(
        &cx,
        activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reserve");
    let (mut staged, _) = admit_staged(&cx, &root, &reservation, run_id(122));
    let new_effect = |run: &Run<'_>| run::EffectSpec {
        target: run.fence().clone(),
        kind: "fake.shared-owner-mutation".into(),
        payload: serde_json::Value::Null,
        settlement: run::Settlement::Retain,
        required: false,
    };
    assert!(
        staged.intend(new_effect(&staged)).is_err(),
        "a staged run cannot act on its future Task fence"
    );
    assert!(staged.effects().expect("effects").is_empty());
    end_run(
        &cx,
        staged,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("release failed staging");
    let (_, _successor, _, _activation) =
        activate(&cx, &root, &environment, run_id(123), run_id(124));
    let effects_before = predecessor.effects().expect("existing effects").len();
    assert!(
        predecessor.intend(new_effect(&predecessor)).is_err(),
        "a retired session cannot create new shared-owner effects"
    );
    assert_eq!(
        predecessor.effects().expect("existing effects").len(),
        effects_before
    );
    assert!(
        predecessor.requests().is_ok(),
        "run-bound lifecycle still works"
    );
    end_run(
        &cx,
        predecessor,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn an_older_activation_table_migrates_its_session_owner_on_write() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("root");
    let (environment, fence) = active_environment(&cx, &root);
    let (session_run, session) = admit_session(&cx, &root, &fence);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reserve");
    let mut store = Store::open(&cx, &root).expect("store");
    store.transaction(&cx, |txn| {
        txn.conn().execute("UPDATE workspace_activation_reservations SET session_run=?1,session_generation=?2 WHERE reservation_id=?3",
            params![session.run().to_string(), session.generation().get() as i64, reservation.id().to_string()])?;
        txn.conn().execute_batch("ALTER TABLE workspace_activation_reservations DROP COLUMN session_owner")?;
        Ok::<_, RunError>(())
    }).expect("old activation schema");
    drop(store);
    drop(session_run);
    Store::open(&cx, &root).expect("migrate old schema");
    let recovered =
        WorkspaceActivationReservation::recover(&read_journal(&cx, &root), &environment)
            .verified()
            .cloned()
            .flatten()
            .expect("recovered record");
    assert_eq!(
        recovered.record().session.as_ref().expect("binding").owner,
        workspace_entry()
    );
}

#[test]
fn checkout_aliases_and_other_runs_cannot_consume_a_frozen_workspace_identity() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reserve");
    stage_successor(&cx, &root, &reservation, run_id(140));
    let alias = workspace_entry()
        .with_checkout(CheckoutId::new(CheckoutKind::Primary, [8; 16]))
        .expect("alias");
    assert!(
        Run::begin(
            &cx,
            &root,
            &inv(alias, "workspace"),
            opts(run_id(141)),
            &cleanup(&cx),
            &mut adapter()
        )
        .is_err(),
        "a checkout spelling cannot bypass exclusive staging"
    );
    assert!(
        Run::begin(
            &cx,
            &root,
            &inv(task("other-task"), "other-task"),
            opts(run_id(140)),
            &cleanup(&cx),
            &mut adapter()
        )
        .is_err(),
        "a different owner cannot consume the frozen RunId"
    );
    assert!(run::summary(&read_journal(&cx, &root), run_id(140))
        .verified()
        .is_none());
}
