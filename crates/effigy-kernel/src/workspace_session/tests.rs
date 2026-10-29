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
fn a_settled_or_superseded_session_does_not_resolve() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);

    // A superseded environment generation revokes the recorded session.
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
        None,
        "a stale session is absent, never mistaken for the live one"
    );
    assert_eq!(
        SessionRef::of_run(&reader, session.run())
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "the run's own identity is still readable"
    );

    // A settled session does not resolve either.
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
// transaction or not at all.

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
    let fence = run.fence().clone();
    Attachments::guest_published(&mut run, &ticket, receipt(&fence)).expect("publication receipt");
    run
}

/// The reserved session: the environment's one `Detached` workspace session,
/// admitted and staged against E with `Gate(Env)` free.
fn admit_staged<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    reservation: &WorkspaceActivationReservation,
    id: RunId,
) -> (Run<'a>, SessionRef) {
    SessionRef::admit_reserved(
        cx,
        root,
        &inv(workspace_entry(), "workspace"),
        opts(id),
        &cleanup(cx),
        &mut adapter(),
        reservation,
        Placement::Detached,
    )
    .expect("reserved workspace session")
}

/// One complete activation: reserve, stage the session, admit the environment
/// run at E, publish, commit. Returns the session and the environment run.
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
    assert_eq!(reservation.state(), ActivationReservationState::Reserved);

    let (session_run, session) = admit_staged(&cx, &root, &reservation, run_id(10));
    assert_eq!(
        session.generation(),
        Generation::new(1),
        "S is the session's own generation"
    );
    assert_eq!(
        session.environment_generation(),
        Generation::new(2),
        "E labels the session"
    );
    assert_ne!(session.generation(), session.environment_generation());

    // Staging alone commits nothing: the environment is not active at E.
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        None,
        "an uncommitted session does not resolve"
    );
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(
        attachment_row(&reader, &environment, session_run.id()),
        None
    );
    let plan = reservation
        .plan(&reader)
        .verified()
        .cloned()
        .flatten()
        .expect("staged plan");
    assert_eq!(plan.environment_generation(), Generation::new(2));
    assert_eq!(plan.session_generation(), Generation::new(1));
    assert_eq!(plan.session_run(), session_run.id());

    let environment_run = admit_activation(&cx, &root, &reservation, run_id(11));
    assert_eq!(environment_run.fence().generation, Generation::new(2));
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("commit");
    assert_eq!(committed.session(), session);

    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone()),
        "the committed session resolves as the environment's session"
    );
    // Exactly one mapping, one attachment at E, one attachment effect, and an
    // environment that is active at E with E as its applied generation.
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
    // A generation another run consumed is never reserved again.
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
fn a_competing_session_admission_never_displaces_the_staged_one() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (staged, session) = admit_staged(&cx, &root, &reservation, run_id(30));
    let competing = SessionRef::admit_reserved(
        &cx,
        &root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(31)),
        &cleanup(&cx),
        &mut adapter(),
        &reservation,
        Placement::Detached,
    );
    match competing {
        Err(RunError::Refused(text)) => assert!(
            text.contains("live staged workspace session"),
            "the refusal names the staged session: {text}"
        ),
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("a competing session must not be admitted"),
    }
    // The staged session is untouched and the competing run was never admitted.
    assert!(is_live(&staged));
    let reader = read_journal(&cx, &root);
    assert!(
        run::summary(&reader, run_id(31)).verified().is_none(),
        "a refused competing admission writes no run"
    );
    assert_eq!(
        reservation
            .plan(&reader)
            .verified()
            .cloned()
            .flatten()
            .map(|plan| plan.session_run()),
        Some(session.run())
    );
}

#[test]
fn staging_refuses_a_generation_the_session_run_does_not_own() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (session_run, session) = admit_staged(&cx, &root, &reservation, run_id(32));
    // E is not S: staging the session at E would equate the two fences.
    let error = reservation
        .stage_session_binding(&cx, &root, &session_run, Generation::new(2))
        .expect_err("E is not the session's own generation");
    assert!(matches!(error, RunError::Refused(_)));
    assert_eq!(session.generation(), Generation::new(1));
    let reader = read_journal(&cx, &root);
    assert_eq!(
        reservation
            .plan(&reader)
            .verified()
            .cloned()
            .flatten()
            .map(|plan| plan.session_generation()),
        Some(Generation::new(1)),
        "the staged binding still names S"
    );
}

#[test]
fn a_receipt_is_validated_against_the_staged_plan_before_the_commit() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (_session_run, session) = admit_staged(&cx, &root, &reservation, run_id(33));
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(34));
    let reader = read_journal(&cx, &root);
    let plan = reservation
        .plan(&reader)
        .verified()
        .cloned()
        .flatten()
        .expect("staged plan");

    // The reversible failing safety assertion: a receipt naming the session's
    // own generation S is refused, and the same receipt naming E is accepted.
    let mut wrong = receipt(&plan.fence());
    wrong.environment.generation = plan.session_generation();
    assert_ne!(plan.session_generation(), plan.environment_generation());
    assert!(
        plan.validate_receipt(&wrong).is_err(),
        "a receipt for S must not validate a commit at E"
    );
    assert!(plan.validate_receipt(&receipt(&plan.fence())).is_ok());

    // And the commit validates the receipt the run recorded against that same
    // plan: the published fence is E, so this activation may commit.
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
    let (session_run, session) = admit_staged(&cx, &root, &reservation, run_id(36));
    let claim = reservation.claim().expect("claim");
    let mut environment_run = Run::begin_claimed(
        &cx,
        &root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(37)),
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
    // The same run can still commit once its publication is complete.
    let fence = environment_run.fence().clone();
    Attachments::guest_published(&mut environment_run, &ticket, receipt(&fence))
        .expect("publication receipt");
    let committed = reservation
        .commit(&cx, &root, &environment_run)
        .expect("commit after the receipt");
    assert_eq!(committed.session(), session);
}

#[test]
fn a_commit_is_idempotent_for_the_run_that_made_it() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (reservation, _session_run, session, environment_run) =
        activate(&cx, &root, &environment, run_id(40), run_id(41));
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
        activate(&cx, &root, &environment, run_id(42), run_id(43));
    drop(environment_run);
    let error = claimed
        .cancel(&cx, &root, Wait::FailFast)
        .expect_err("an interrupted run row still owns its admission");
    assert!(matches!(error, RunError::Refused(_)));
}

#[test]
fn a_crash_at_each_boundary_is_recoverable_from_the_durable_reservation() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);

    // 1. After the reservation, before any run: the reservation is the only
    //    durable fact and the retry takes the same one.
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let staged_generation = reservation.generation();
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("recovered reservation");
    assert_eq!(reservation.generation(), staged_generation);

    // 2. After the session is admitted and staged, before the environment run
    //    exists: the staged session is interrupted, the reservation unchanged,
    //    and the next admission settles it and stages a successor.
    let (first_session, first) = admit_staged(&cx, &root, &reservation, run_id(50));
    let first_run = first_session.id();
    assert_eq!(first.generation(), Generation::new(1));
    drop(first_session);
    let rescue = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("the same reservation");
    assert_eq!(rescue.generation(), Generation::new(1));
    let (second_session, second) = admit_staged(&cx, &root, &rescue, run_id(51));
    assert_eq!(second.environment_generation(), Generation::new(1));
    assert_eq!(
        second.generation(),
        Generation::new(2),
        "the recovered session is a new run at its own new generation"
    );
    let reader = read_journal(&cx, &root);
    match run::summary(&reader, first_run) {
        Observed::Verified(summary, _) => assert_eq!(summary.phase, Phase::Closed),
        other => panic!("the interrupted session is settled: {other:?}"),
    }
    let plan = rescue
        .plan(&reader)
        .verified()
        .cloned()
        .flatten()
        .expect("the replaced staging");
    assert_eq!(plan.session_run(), second_session.id());

    // 3. Complete the activation, then crash after the pointer and before the
    //    commit: nothing is advertised and the workspace is unavailable.
    let environment_run = admit_activation(&cx, &root, &rescue, run_id(52));
    let uncommitted = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&uncommitted, &environment)
            .verified()
            .cloned()
            .flatten(),
        None,
        "a published pointer without a commit is unavailable"
    );
    assert_eq!(mapping(&uncommitted, &environment), None);
    // The host that crashed owned both runs of that generation.
    drop(environment_run);
    drop(second_session);

    // 4. The retry reserves a new generation, stages a new session and commits.
    let next = WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
        .expect("the next generation");
    assert_eq!(
        next.generation(),
        Generation::new(2),
        "the generation a run took is never reissued"
    );
    // The stale reservation keeps its evidence and cannot be claimed again.
    let stale_claim = rescue
        .claim()
        .expect("the stale reservation is still readable");
    let refused = Run::begin_claimed(
        &cx,
        &root,
        &inv(environment.clone(), "environment:up"),
        opts(run_id(53)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        stale_claim,
    );
    match refused {
        Err(RunError::Refused(text)) => assert!(
            text.contains("not admitted at the generation"),
            "a consumed generation refuses the claim: {text}"
        ),
        Err(other) => panic!("expected a refusal, got {other:?}"),
        Ok(_) => panic!("a consumed generation must not be claimed again"),
    }
    let (session_run, session) = admit_staged(&cx, &root, &next, run_id(54));
    let environment_run = admit_activation(&cx, &root, &next, run_id(55));
    next.commit(&cx, &root, &environment_run)
        .expect("the recovered activation commits");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone())
    );
    assert_eq!(session.environment_generation(), Generation::new(2));
    assert_eq!(session_run.fence().generation, session.generation());
}

#[test]
fn a_successor_preserves_a_live_holder_and_the_predecessor_session_contract() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);

    // Generation 1 activates with its own session.
    let (_reservation, predecessor, first, first_activation) =
        activate(&cx, &root, &environment, run_id(60), run_id(61));
    // The activation run settles as the engine's `finish` does, so the next
    // generation's gate and holders are not blocked by a retired run.
    let mut first_activation = first_activation;
    first_activation
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt the activation");
    first_activation
        .settle(&cleanup(&cx), &mut adapter())
        .expect("settle the activation");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(first.clone())
    );
    // A live holder of the predecessor generation: a host `dev` run that
    // borrows the environment. A successor must leave it exactly as it was.
    let mut host = Run::begin(
        &cx,
        &root,
        &inv(task("dev:host"), "dev:host"),
        opts(run_id(62)),
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

    // The predecessor session is still live, so a successor session is refused
    // rather than admitted: one owner admits one unresolved run (K5), and a
    // successor never stops the predecessor to take its place.
    let successor =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("successor reservation");
    assert_eq!(successor.generation(), Generation::new(2));
    match SessionRef::admit_reserved(
        &cx,
        &root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(63)),
        &cleanup(&cx),
        &mut adapter(),
        &successor,
        Placement::Detached,
    ) {
        Err(RunError::Gate(_)) => {}
        Err(other) => panic!("expected the predecessor's owner gate, got {other:?}"),
        Ok(_) => panic!("a second session must not be admitted while one is live"),
    }
    assert!(
        is_live(&predecessor),
        "the predecessor session is not stopped"
    );
    assert_eq!(
        stop_requests(&cx, &root, predecessor.id()),
        0,
        "no stop was requested"
    );

    // Once the predecessor session ended, the successor stages and commits, and
    // the live holder keeps its own generation and attachment.
    let mut predecessor = predecessor;
    predecessor
        .interrupt(
            &cleanup(&cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("stop the predecessor session");
    predecessor
        .settle(&cleanup(&cx), &mut adapter())
        .expect("settle the predecessor session");
    let (successor_session_run, successor_session) =
        admit_staged(&cx, &root, &successor, run_id(64));
    let environment_run = admit_activation(&cx, &root, &successor, run_id(65));
    successor
        .commit(&cx, &root, &environment_run)
        .expect("successor commit");

    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
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
    // The predecessor's record is revoked by the new generation: it settled,
    // so its attachment is released and it is no session to attach to.
    assert_eq!(
        SessionRef::of_run(&reader, first.run())
            .verified()
            .cloned()
            .flatten(),
        None
    );
}
