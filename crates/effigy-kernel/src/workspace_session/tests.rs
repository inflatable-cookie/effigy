//! Fake-owner and fake-runtime proofs for the workspace session (§8.7).
//!
//! These tests drive the session layer a supervisor and a dispatcher use: one
//! environment-owned `Detached` session run, one identity for every front end,
//! the down a session requests from inside, and the loss a terminal reports.
//! The replay-safe admission record, the durable attachments and the bounded
//! stream cursor belong to the kernel workspace seam (`crate::run::workspace`)
//! and are proven there; a session consumes them. No engine, container,
//! gateway or socket server is involved: the "fake owner" is a real kernel
//! `Run` in a fixture journal, and the "fake runtime" records nothing. The
//! detached-custody regression re-execs a disposable sentinel to prove lifecycle
//! registration with the actual custody plan.

use super::selection::{BINARY_SELECTOR, CONTEXT_SELECTOR, ENTRY_SELECTOR};
use super::*;
use crate::ctx::{
    CancelToken, Deadline, EffectClass, EffectContext, Limits, OperationContext, StopCause,
};
use crate::gate::OwnerState;
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
    let (mut run, session) = SessionRef::admit(
        cx,
        root,
        &inv(workspace_entry(), "workspace"),
        opts(run_id(1)),
        &cleanup(cx),
        &mut adapter(),
        environment,
        Placement::Detached,
    )
    .expect("workspace session");
    run.register_session_custody("supervisor-1")
        .expect("detached custody");
    (run, session)
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
    let (mut _session_run, session) = admit_session(&cx, &root, &fence);

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
    let (mut _session_run, session) = admit_session(&cx, &root, &fence);
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
    let (mut _session_run, session) = admit_session(&cx, &root, &fence);
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
    assert!(SessionRef::of_run(&reader, run_id(31)).verified().is_none());
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
    let (mut _session_run, session) = admit_session(&cx, &root, &fence);

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
    assert!(SessionRef::of_run(&reader, run_id(32)).verified().is_none());
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
    let (mut admitted, session) = SessionRef::admit_reserved(
        cx,
        root,
        &inv(workspace_entry(), "workspace"),
        opts(run),
        &cleanup(cx),
        &mut adapter(),
        reservation,
        Placement::Detached,
    )
    .expect("reserved workspace session");
    admitted
        .register_session_custody("supervisor-1")
        .expect("detached custody");
    (admitted, session)
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
    stage_fake_binary(reservation, &mut run, fake_artifact()).expect("stage fake binary");
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
    let reservation = decided(cx, root, environment);
    let (mut session_run, session) = admit_staged(cx, root, &reservation, session_id);
    let mut environment_run = admit_activation(cx, root, &reservation, activation_id);
    let (receipt, launch) =
        commit_receipt(cx, root, &reservation, &environment_run, &mut session_run)
            .expect("activation receipt");
    let committed = reservation
        .commit_selection(cx, root, &environment_run, &session_run, &receipt, &launch)
        .expect("activation commit");
    // A completed activation switches its selectors: the generation it
    // published is attachable only once its selection is finished.
    let mut reader = FakeSelectors::exact(&receipt);
    Attachments::selection_switched(
        &mut environment_run,
        committed
            .selection_obligation()
            .expect("the commit records its selection"),
        &launch,
        &mut reader,
    )
    .expect("the switch is recorded");
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
    let reservation = decided(&cx, &root, &environment);
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
    let (mut session_run, session) = admit_frozen(&cx, &root, &reservation, run_id(10));
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
    let committed = commit_activation(&cx, &root, &reservation, &environment_run, &mut session_run)
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
    let reservation = decided(&cx, &root, &environment);
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
    let reservation = decided(&cx, &root, &environment);
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
    let reservation = decided(&cx, &root, &environment);
    let binding = stage_successor(&cx, &root, &reservation, run_id(50));
    let (mut session_run, _session) = admit_frozen(&cx, &root, &reservation, binding.run);
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
    let mut environment_run = environment_run;
    let committed = commit_and_switch(
        &cx,
        &root,
        &reservation,
        &mut environment_run,
        &mut session_run,
    )
    .expect("the frozen activation commits");
    assert_eq!(committed.session().run(), binding.run);
}

#[test]
fn a_receipt_is_validated_against_the_frozen_plan_before_the_commit() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation = decided(&cx, &root, &environment);
    let (mut session_run, session) = admit_staged(&cx, &root, &reservation, run_id(53));
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
    let mut environment_run = environment_run;
    let committed = commit_and_switch(
        &cx,
        &root,
        &reservation,
        &mut environment_run,
        &mut session_run,
    )
    .expect("the activation commits at E");
    assert_eq!(committed.session(), session);
}

#[test]
fn a_commit_without_an_applied_publication_writes_nothing() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation = decided(&cx, &root, &environment);
    let (mut session_run, session) = admit_staged(&cx, &root, &reservation, run_id(55));
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
    let error = commit_activation(&cx, &root, &reservation, &environment_run, &mut session_run)
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
    stage_fake_binary(&reservation, &mut environment_run, fake_artifact())
        .expect("stage fake binary");
    // The same run can still commit once its publication is complete.
    let receipt = publication_for(&cx, &root, &reservation, &environment_run);
    Attachments::workspace_guest_published(&mut environment_run, &ticket, receipt)
        .expect("publication receipt");
    let mut environment_run = environment_run;
    let committed = commit_and_switch(
        &cx,
        &root,
        &reservation,
        &mut environment_run,
        &mut session_run,
    )
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
    let reservation = decided(&cx, &root, &environment);
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(57));
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
    let error = commit_activation(&cx, &root, &reservation, &environment_run, &mut session_run)
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
    let error = commit_activation(&cx, &root, &reservation, &environment_run, &mut session_run)
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
    let (reservation, mut session_run, session, environment_run) =
        activate(&cx, &root, &environment, run_id(60), run_id(61));
    let reader = read_journal(&cx, &root);
    let recovered = WorkspaceActivationReservation::recover(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("the committed reservation");
    let again = commit_activation(&cx, &root, &recovered, &environment_run, &mut session_run)
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
            WorkspaceActivationReservation::insert(txn, &environment, Generation::new(3), None)
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
    let next_reservation = decided(&cx, &root, &environment);
    assert_eq!(next_reservation.generation(), Generation::new(3));
    let next_binding = stage_successor(&cx, &root, &next_reservation, run_id(74));
    let (mut successor_run, successor) =
        admit_frozen(&cx, &root, &next_reservation, next_binding.run);
    let mut environment_run = admit_activation(&cx, &root, &next_reservation, run_id(75));
    let committed = commit_and_switch(
        &cx,
        &root,
        &next_reservation,
        &mut environment_run,
        &mut successor_run,
    )
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
    let successor_reservation = decided(&cx, &root, &environment);
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
    let (mut successor_session_run, successor_session) =
        admit_frozen(&cx, &root, &successor_reservation, binding.run);
    let mut environment_run = admit_activation(&cx, &root, &successor_reservation, run_id(84));
    commit_and_switch(
        &cx,
        &root,
        &successor_reservation,
        &mut environment_run,
        &mut successor_session_run,
    )
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
            // The decision itself is one of the boundaries: a fault there must
            // be reported, not panic, so the loop drives `decide` directly.
            let reservation = match WorkspaceActivationReservation::decide(
                &cx,
                &root,
                &environment,
                &decision_inputs(70),
                Wait::FailFast,
            )? {
                ActivationDecision::Activate(reservation) => *reservation,
                ActivationDecision::Attach(_) => {
                    return Err(RunError::broken("a fresh generation cannot attach"))
                }
            };
            reservation.stage_session_binding(&cx, &root, &workspace_entry(), run_id(102))?;
            let (mut run, _) = SessionRef::admit_reserved(
                &cx,
                &root,
                &inv(workspace_entry(), "workspace"),
                opts(run_id(102)),
                &cleanup(&cx),
                &mut adapter(),
                &reservation,
                Placement::Detached,
            )?;
            run.register_session_custody("supervisor-1")
                .expect("detached custody");
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
            stage_fake_binary(&reservation, run, fake_artifact())?;
            let ticket = Attachments::intend_guest_publication(run)?;
            let receipt = publication_for(&cx, &root, &reservation, run);
            Attachments::workspace_guest_published(run, &ticket, receipt)?;
            commit_and_switch(
                &cx,
                &root,
                &reservation,
                run,
                staged_run.as_mut().expect("owning successor"),
            )?;
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
        if committed {
            let obligation =
                una_finished(&read_journal(&cx, &root), &environment).expect("pending");
            let dead = SelectionLiveness {
                session: Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "runs".into(),
                }),
                target: Observed::Verified((), evidence()),
                listener: Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "listener".into(),
                }),
            };
            selection_abandoned(
                &cx,
                &root,
                &obligation,
                &dead,
                "settled dead successor",
                Wait::FailFast,
            )
            .expect("abandon after settlement");
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

#[test]
fn cancellation_waits_for_stopping_and_incomplete_runs_to_settle() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("root");
    let environment = pending_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reserve");
    let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(150));
    let mut activation = admit_activation(&cx, &root, &reservation, run_id(151));
    let cancelled = || WorkloadOutcome::Cancelled {
        cause: StopCause::Requested,
        signal: None,
    };
    activation
        .interrupt(&cleanup(&cx), cancelled())
        .expect("activation stopping");
    drop(activation);
    assert!(
        reservation.cancel(&cx, &root, Wait::FailFast).is_err(),
        "a dropped Stopping activation still needs settlement"
    );
    run::settle_interrupted(
        &cx,
        &root,
        &environment,
        run_id(151),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("settle activation");
    session_run
        .interrupt(&cleanup(&cx), cancelled())
        .expect("session stopping");
    assert!(
        reservation.cancel(&cx, &root, Wait::FailFast).is_err(),
        "a live Stopping session still needs settlement"
    );
    drop(session_run);
    assert!(
        reservation.cancel(&cx, &root, Wait::FailFast).is_err(),
        "a dropped Stopping session still needs settlement"
    );
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(150),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("settle session");
    Store::open(&cx, &root)
        .expect("store")
        .transaction(&cx, |txn| {
            txn.conn().execute(
                "UPDATE runs SET cleanup_outcome='incomplete' WHERE run_id=?1",
                [run_id(150).to_string()],
            )?;
            Ok::<_, RunError>(())
        })
        .expect("closed but incomplete cleanup");
    assert!(
        reservation.cancel(&cx, &root, Wait::FailFast).is_err(),
        "Closed with Incomplete cleanup is not complete settlement"
    );
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(150),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("retry incomplete settlement");
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("cancel after complete settlement");
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_reserved_successor_registers_real_detached_custody_under_its_private_slot() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let (_, mut predecessor, first, activation) =
        activate(&cx, &root, &environment, run_id(140), run_id(141));
    end_run(
        &cx,
        activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let reservation = decided(&cx, &root, &environment);
    stage_successor(&cx, &root, &reservation, run_id(142));
    let mut options = opts(run_id(142));
    options.custody = Some(crate::supervise::LifecyclePlan {
        executable: crate::supervise::Executable::capture(&crate::fixture::custody_fixture())
            .expect("custody fixture"),
        placement: Placement::Detached,
    });
    let (mut session_run, session) = SessionRef::admit_reserved(
        &cx,
        &root,
        &inv(workspace_entry(), "workspace"),
        options,
        &cleanup(&cx),
        &mut adapter(),
        &reservation,
        Placement::Detached,
    )
    .expect("actual detached successor admission");
    session_run
        .register_session_custody("supervisor-1")
        .expect("supervisor adopted");
    let metadata = run::metadata(&observe(&cx), &root, session_run.id());
    let metadata = metadata.verified().expect("registered lifecycle");
    assert_eq!(metadata.placement, "detached");
    assert!(metadata.owner.is_some());
    assert!(metadata.supervisor.is_some());
    assert_eq!(metadata.sentinel_status, run::SentinelStatus::Live);
    assert_eq!(
        resolved(&read_journal(&cx, &root), &environment),
        Some(first)
    );
    let mut activation = admit_activation(&cx, &root, &reservation, run_id(143));
    assert!(
        commit_activation(&cx, &root, &reservation, &activation, &mut predecessor).is_err(),
        "a different owning Run cannot stand in for the frozen successor"
    );
    commit_and_switch(&cx, &root, &reservation, &mut activation, &mut session_run)
        .expect("commit with owned slot");
    assert_eq!(
        resolved(&read_journal(&cx, &root), &environment),
        Some(session)
    );
    end_run(
        &cx,
        activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
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
fn a_failed_successor_keeps_the_predecessor_fence_usable_for_new_borrowed_work() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let (_, predecessor, first, activation) =
        activate(&cx, &root, &environment, run_id(150), run_id(151));
    end_run(
        &cx,
        activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let reservation = decided(&cx, &root, &environment);
    let (successor, _) = admit_staged(&cx, &root, &reservation, run_id(152));
    let mut activation = Run::begin_claimed(
        &cx,
        &root,
        &inv(environment.clone(), "environment:up"),
        opts(run_id(153)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        reservation.claim().expect("claim"),
    )
    .expect("successor Env admission advances transition fence");
    let ticket =
        Attachments::intend_guest_publication(&mut activation).expect("publication intent");
    Attachments::guest_publication_unknown(&mut activation, &ticket, UnknownReason::Injected)
        .expect("uncertain publication");
    drop(activation); // crash before commit; the successor never becomes active
    let fence = Fence {
        resource: environment.clone(),
        generation: first.environment_generation(),
    };
    let mut holder = Run::begin(
        &cx,
        &root,
        &inv(task("predecessor-operation"), "predecessor-operation"),
        opts(run_id(154)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("new predecessor operation");
    Attachments::attach(&mut holder, &fence, AttachMode::Borrow, Wait::FailFast)
        .expect("published predecessor remains usable after failed activation");
    let remote = holder
        .intend_remote(EffectSpec {
            target: fence.clone(),
            kind: crate::remote::KIND.into(),
            payload: serde_json::Value::Null,
            settlement: Settlement::Retain,
            required: false,
        })
        .expect("borrowed remote intent uses the predecessor's applied fence");
    holder
        .applied(&remote, serde_json::Value::Null)
        .expect("remote callback");
    assert_eq!(
        resolved(&read_journal(&cx, &root), &environment),
        Some(first.clone())
    );
    // Borrowing authority never authorizes an environment transition, nor
    // the unpublished E. A distinct holder tests each refusal without poison reuse.
    for (id, mode, generation) in [
        (
            155,
            AttachMode::Exclusive {
                stop_on_release: true,
            },
            fence.generation,
        ),
        (156, AttachMode::Borrow, reservation.generation()),
    ] {
        let mut other = Run::begin(
            &cx,
            &root,
            &inv(task(&format!("other-{id}")), "other"),
            opts(run_id(id)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("other holder");
        assert!(Attachments::attach(
            &mut other,
            &Fence {
                resource: environment.clone(),
                generation
            },
            mode,
            Wait::FailFast
        )
        .is_err());
    }
    // Explicit recovery/cancellation still leaves the predecessor usable.
    drop(successor);
    for (owner, id) in [
        (environment.clone(), run_id(153)),
        (workspace_entry(), run_id(152)),
    ] {
        run::settle_interrupted(
            &cx,
            &root,
            &owner,
            id,
            next(),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("recover failed successor");
    }
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("cancel failed activation");
    // EnvRun::finish uses publish_active after HeldByRuns without applying.
    // A later ordinary activate uses publish_active_for. Neither owns a new
    // workspace session or may retarget the still-published predecessor.
    for id in [160, 162] {
        let mut borrower = Run::begin(
            &cx,
            &root,
            &inv(task(&format!("held-operation-{id}")), "held-operation"),
            opts(run_id(id)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("fresh borrower");
        let mut ordinary = Run::begin(
            &cx,
            &root,
            &inv(environment.clone(), "environment:up"),
            opts(run_id(id + 1)),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("ordinary held reconcile after cancellation");
        assert!(ordinary.fence().generation > reservation.generation());
        if id == 160 {
            Attachments::publish_active(&mut ordinary).expect("no-op held publication");
        } else {
            Attachments::publish_active_for(&mut ordinary, &[(borrower.id(), AttachMode::Borrow)])
                .expect("activate borrows the preserved fence");
        }
        ordinary
            .finish(WorkloadOutcome::Succeeded)
            .expect("finish no-op");
        ordinary
            .settle(&cleanup(&cx), &mut adapter())
            .expect("settle no-op");
        let reader = read_journal(&cx, &root);
        let applied: String = reader
            .conn()
            .query_row(
                "SELECT last_applied FROM resources WHERE name=?1 AND state='active'",
                [environment.name().to_string()],
                |row| row.get(0),
            )
            .expect("applied predecessor generation");
        assert_eq!(applied, fence.generation.get().to_string());
        assert_eq!(
            SessionRef::resolve(&reader, &environment).verified(),
            Some(&Some(first.clone()))
        );
        if id == 160 {
            Attachments::attach(&mut borrower, &fence, AttachMode::Borrow, Wait::FailFast)
                .expect("new Borrow remains usable after ordinary held reconciliation");
        }
        borrower
            .intend_remote(EffectSpec {
                target: fence.clone(),
                kind: crate::remote::KIND.into(),
                payload: serde_json::Value::Null,
                settlement: Settlement::Retain,
                required: false,
            })
            .expect("remote intent uses the unchanged predecessor fence");
        assert!(borrower
            .effects()
            .expect("effects")
            .iter()
            .all(|effect| effect.spec.target == fence));
        borrower
            .finish(WorkloadOutcome::Succeeded)
            .expect("finish borrower");
        borrower
            .settle(&cleanup(&cx), &mut adapter())
            .expect("settle borrower");
    }
    holder
        .intend_remote(EffectSpec {
            target: fence.clone(),
            kind: crate::remote::KIND.into(),
            payload: serde_json::Value::Null,
            settlement: Settlement::Retain,
            required: false,
        })
        .expect("cancel does not revoke published predecessor authority");
    // Once a successor atomically commits, the exception cannot start more
    // work against the old environment fence, even for its existing borrower.
    let (_, _next_session, _, _activation) =
        activate(&cx, &root, &environment, run_id(157), run_id(158));
    assert!(
        holder
            .intend_remote(EffectSpec {
                target: fence,
                kind: crate::remote::KIND.into(),
                payload: serde_json::Value::Null,
                settlement: Settlement::Retain,
                required: false,
            })
            .is_err(),
        "committed replacement ends the predecessor exception"
    );
    drop(predecessor);
}

/// The staged owner is borrowed through commit, so its process cannot die
/// independently of that commit. Kill it at the former probe/transaction gap.
#[cfg(feature = "fault-injection")]
#[test]
fn successor_death_at_the_commit_boundary_cannot_publish_a_dead_session() {
    const CHILD_HOME: &str = "EFFIGY_WORKSPACE_COMMIT_DEATH_HOME";
    let cx = context();
    if let Some(home) = std::env::var_os(CHILD_HOME) {
        let root = StateRoot::effects(&cx, std::path::Path::new(&home)).expect("child state root");
        // Inputs no other process decided: this child must reserve a successor,
        // never attach to the predecessor the parent left behind.
        let reservation = decided_with(&cx, &root, &environment(), 200);
        let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(162));
        let activation = admit_activation(&cx, &root, &reservation, run_id(163));
        crate::fault::arm_point(
            crate::fault::Point::ActivationCommitBefore,
            crate::fault::Fault::Kill,
        );
        let _ = commit_activation(&cx, &root, &reservation, &activation, &mut session_run);
        panic!("the owner must die before committing");
    }
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let (_, predecessor, first, activation) =
        activate(&cx, &root, &environment, run_id(160), run_id(161));
    end_run(
        &cx,
        activation,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .args(["--exact", "workspace_session::tests::successor_death_at_the_commit_boundary_cannot_publish_a_dead_session", "--nocapture"])
        .env(CHILD_HOME, fixture.path())
        .status().expect("re-exec owner");
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(libc::SIGKILL)); // status() reaps this exact child
    let reader = read_journal(&cx, &root);
    assert_eq!(resolved(&reader, &environment), Some(first.clone()));
    assert_eq!(resource_row(&reader, &workspace_entry()).0, 1);
    assert_eq!(attachment_row(&reader, &environment, run_id(162)), None);
    assert!(matches!(
        reader.run_owner_state(&observe(&cx), &root, &run_id(162)),
        Observed::Verified(OwnerState::Interrupted, _)
    ));
    let reservation = WorkspaceActivationReservation::recover(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("recover dead activation");
    // Both successor owners died before commit; the predecessor still works.
    let mut holder = Run::begin(
        &cx,
        &root,
        &inv(task("after-owner-death"), "after-owner-death"),
        opts(run_id(164)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("predecessor operation");
    Attachments::attach(
        &mut holder,
        &Fence {
            resource: environment.clone(),
            generation: first.environment_generation(),
        },
        AttachMode::Borrow,
        Wait::FailFast,
    )
    .expect("predecessor works after successor death");
    for (owner, run) in [(environment, run_id(163)), (workspace_entry(), run_id(162))] {
        run::settle_interrupted(
            &cx,
            &root,
            &owner,
            run,
            next(),
            &cleanup(&cx),
            &mut adapter(),
        )
        .expect("recover exact dead run");
    }
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("release dead reservation");
    assert_eq!(stop_requests(&cx, &root, first.run()), 0);
    end_run(
        &cx,
        holder,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
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
fn generic_active_publication_cannot_bypass_a_reserved_session_commit() {
    for with_holders in [false, true] {
        let cx = context();
        let fixture = home();
        let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
        let environment = pending_environment(&cx, &root);
        let (_, _predecessor, first, activation) =
            activate(&cx, &root, &environment, run_id(170), run_id(171));
        end_run(
            &cx,
            activation,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        );
        let reservation = decided(&cx, &root, &environment);
        let (_successor, session) = admit_staged(&cx, &root, &reservation, run_id(172));
        let mut activation = admit_activation(&cx, &root, &reservation, run_id(173));
        let result = if with_holders {
            Attachments::publish_active_for(&mut activation, &[(session.run(), AttachMode::Borrow)])
        } else {
            Attachments::publish_active(&mut activation)
        };
        assert!(
            result.is_err(),
            "reserved activation requires the atomic session commit"
        );
        let reader = read_journal(&cx, &root);
        assert_eq!(resolved(&reader, &environment), Some(first));
        assert_eq!(
            resource_row(&reader, &environment),
            (2, Some("pending".into()), Some("1".into()))
        );
        assert_eq!(attachment_row(&reader, &environment, session.run()), None);
    }
}

// ---- 052 §8.11 C1: the atomic activation decision -------------------------
//
// The decision is the one place an attach and a reservation are the same
// question. Every test here drives the real journal, the real `Gate(Env)` and
// the real reservation table; the "concurrent actor" is a second caller of the
// same API, not a second authority.

/// The container and boot the fake guest publication names. A decision that
/// re-observes this target is attachable; any other observation is not.
fn published_target() -> ContainerIdentity {
    ContainerIdentity {
        container_id: "a1".repeat(32),
        vm_boot_id: "boot-0001".to_owned(),
        backend: "docker-desktop".to_owned(),
    }
}

fn artifact(byte: u8) -> Digest {
    Digest::of(&[byte, b'a'])
}

/// The frozen inputs a decision compares. `byte` changes every digest, so two
/// calls with different bytes are two different publish intents.
fn publish_inputs(byte: u8, target: Option<ContainerIdentity>) -> PublishInputs {
    let digest = artifact(byte);
    let object = format!("/run/effigy/objects/bin/{digest}");
    PublishInputs {
        platform: None,
        guest: Some(fake_guest_context()),
        artifact: ArtifactEvidence::Selected {
            digest: digest.clone(),
            object,
        },
        // The resolved selection a decision freezes is the context document the
        // generation serves.
        selection: Digest::of(&[byte, b's', b'e', b'l']),
        inputs_digest: Digest::of(&[byte, b'i']),
        target: match target {
            Some(identity) => TargetEvidence::Observed(identity),
            None => TargetEvidence::Absent {
                project: "fixture".to_owned(),
                backend: "docker-desktop".to_owned(),
            },
        },
    }
}

/// The environment activation run at E with the exact artifact digest a
/// selection names published, so `commit_selection` validates rather than
/// merely records.
fn admit_activation_publishing<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    reservation: &WorkspaceActivationReservation,
    id: RunId,
    binary: &Digest,
) -> (Run<'a>, WorkspacePublicationReceipt) {
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
    stage_fake_binary(reservation, &mut run, binary.clone()).expect("stage fake binary");
    let ticket = Attachments::intend_guest_publication(&mut run).expect("publication intent");
    let plan = plan_of(reservation, &read_journal(cx, root)).expect("staged plan");
    let publication = WorkspacePublicationReceipt {
        publication: run::GuestPublicationReceipt {
            environment: plan.fence(),
            container_id: "a1".repeat(32),
            vm_boot_id: "boot-0001".to_owned(),
            artifact_digest: format!("sha256:{}", binary.hex()),
            context_generation: 1,
        },
        activation_run: run.id(),
        session: plan.binding().clone(),
    };
    Attachments::workspace_guest_published(&mut run, &ticket, publication.clone())
        .expect("publication receipt");
    (run, publication)
}

/// The artifact every fake guest publication in this module names.
fn fake_artifact() -> Digest {
    Digest::parse_hex(&"b2".repeat(32)).expect("fixed artifact digest")
}

thread_local! {
    /// Fresh publish inputs per decision in a test, so a second activation in
    /// the same test is a successor, never an accidental attach.
    static DECISION_BYTE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

/// Frozen inputs for one test activation, naming the artifact the fake
/// publication acknowledges so the commit's frozen-input check passes.
fn decision_inputs(byte: u8) -> PublishInputs {
    let digest = fake_artifact();
    let object = format!("/run/effigy/objects/bin/{digest}");
    PublishInputs {
        platform: None,
        guest: Some(fake_guest_context()),
        artifact: ArtifactEvidence::Selected {
            digest: digest.clone(),
            object,
        },
        selection: Digest::of(&[byte, b's', b'e', b'l']),
        inputs_digest: Digest::of(&[byte, b'i', b'n']),
        target: TargetEvidence::Absent {
            project: "fixture".to_owned(),
            backend: "docker-desktop".to_owned(),
        },
    }
}

/// A reservation made the way production makes one: decided, with its publish
/// inputs frozen before anything is applied.
fn decided(
    cx: &EffectContext,
    root: &StateRoot,
    environment: &ResourceId,
) -> WorkspaceActivationReservation {
    let byte = DECISION_BYTE.with(|cell| {
        let byte = cell.get();
        cell.set(byte.wrapping_add(1));
        byte
    });
    decided_with(cx, root, environment, byte)
}

/// The same decision with explicit frozen inputs, for a process that must not
/// accidentally match a generation another process decided.
fn decided_with(
    cx: &EffectContext,
    root: &StateRoot,
    environment: &ResourceId,
    byte: u8,
) -> WorkspaceActivationReservation {
    match WorkspaceActivationReservation::decide(
        cx,
        root,
        environment,
        &decision_inputs(byte),
        Wait::FailFast,
    )
    .expect("activation decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("a fresh generation cannot attach"),
    }
}

/// The context document the fixture prepare would write: a real v2 document
/// naming the exact generation, session, artifact, credential and locator this
/// activation publishes. The kernel parses and validates it; a document for
/// anything else is refused.
fn context_document(
    plan: &ActivationPlan,
    publication: &WorkspacePublicationReceipt,
    selection: &GuestSelection,
    lease: &ServingLease,
    selection_digest: &Digest,
) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": "effigy.workspace-context.v2",
        "version": plan.guest_context().expect("guest").version,
        "paths": plan.guest_context().expect("guest").paths,
        "sources": plan.guest_context().expect("guest").sources,
        "binary_path": selection.objects.binary.path,
        "backend": selection.backend,
        "context_generation": selection.objects.context_generation.get(),
        "environment_generation": plan.environment_generation().get(),
        "session": {
            "run_id": plan.session_run().to_string(),
            "generation": plan.session_generation().get(),
        },
        "artifact_digest": format!("sha256:{}", selection.objects.binary.digest.hex()),
        "container_id": publication.publication.container_id,
        "vm_boot_id": publication.publication.vm_boot_id,
        "credential": {
            "generation": selection.objects.credential_generation.get(),
            "path": selection.objects.credential.path,
            "digest": selection.objects.credential.digest.hex(),
        },
        "locator": {
            "endpoint": lease.endpoint(),
            "pin": lease.pin().hex(),
        },
        "selection_digest": selection_digest.hex(),
    }))
    .expect("context document")
}

/// A fake reader of the exact target: what a test says the guest holds. The
/// kernel compares it, so a wrong entry is a refusal, not a shortcut.
#[derive(Debug, Default)]
struct FakeSelectors {
    target: Option<ContainerIdentity>,
    selectors: BTreeMap<String, Observed<String>>,
    objects: BTreeMap<String, Observed<Digest>>,
}

impl FakeSelectors {
    fn prior(selection: &GuestSelection) -> Self {
        let mut fake = Self::default();
        for change in &selection.switches {
            fake.selectors.insert(
                change.path.clone(),
                match &change.expected {
                    Some(value) => Observed::Verified(value.clone(), evidence()),
                    None => Observed::Absent(crate::observe::AbsenceProof::NoRow {
                        table: "guest-selector".into(),
                    }),
                },
            );
        }
        fake
    }

    fn exact(receipt: &SelectionReceipt) -> Self {
        let selection = receipt.selection();
        let mut fake = Self::default();
        for change in &selection.switches {
            fake.selectors.insert(
                change.path.clone(),
                Observed::Verified(change.next.clone(), evidence()),
            );
        }
        for object in [
            &selection.objects.binary,
            &selection.objects.context,
            &selection.objects.credential,
        ] {
            fake.objects.insert(
                object.path.clone(),
                Observed::Verified(object.digest.clone(), evidence()),
            );
        }
        fake
    }

    /// A selector holding a value this activation never committed.
    fn third(receipt: &SelectionReceipt) -> Self {
        let mut fake = Self::exact(receipt);
        let path = receipt.selection().switches[0].path.clone();
        fake.selectors.insert(
            path,
            Observed::Verified("a third value".to_owned(), evidence()),
        );
        fake
    }

    /// An object that no longer hashes to what the selection names.
    fn missing_object(receipt: &SelectionReceipt) -> Self {
        let mut fake = Self::exact(receipt);
        fake.objects.insert(
            receipt.selection().objects.context.path.clone(),
            Observed::Absent(crate::observe::AbsenceProof::NoRow {
                table: "guest-object".into(),
            }),
        );
        fake
    }
}

impl SelectorReader for FakeSelectors {
    fn target(&self) -> ContainerIdentity {
        self.target.clone().unwrap_or_else(published_target)
    }
    fn read_selector(&mut self, path: &str) -> Observed<String> {
        self.selectors
            .get(path)
            .cloned()
            .unwrap_or(Observed::Absent(crate::observe::AbsenceProof::NoRow {
                table: "guest-selector".into(),
            }))
    }
    fn read_object(&mut self, path: &str) -> Observed<Digest> {
        self.objects.get(path).cloned().unwrap_or(Observed::Absent(
            crate::observe::AbsenceProof::NoRow {
                table: "guest-object".into(),
            },
        ))
    }
}

/// One session's own listener and key custody, bound and qualified, launched on
/// behalf of the run that owns the session.
/// A fake target probe: what a test says the container and boot are.
#[derive(Debug, Default)]
struct FakeTarget {
    answer: Option<Observed<()>>,
}

impl TargetProbe for FakeTarget {
    fn verify_target(
        &mut self,
        _container_id: &str,
        _vm_boot_id: &str,
        _deadline: Duration,
    ) -> Observed<()> {
        self.answer
            .clone()
            .unwrap_or_else(|| Observed::Unknown(UnknownReason::Unparsable("no target".into())))
    }
}

fn verifying_target() -> FakeTarget {
    FakeTarget {
        answer: Some(Observed::Verified((), evidence())),
    }
}

/// A probe that answers the pinned handshake, for recovery tests.
fn verifying_probe() -> FakeProbe {
    FakeProbe {
        answer: Some(Observed::Verified((), evidence())),
        calls: 0,
    }
}

/// The resolved selection digest a reservation's decision froze.
fn receipt_selection(reservation: &WorkspaceActivationReservation) -> Digest {
    reservation
        .inputs()
        .map(|inputs| inputs.selection.clone())
        .unwrap_or_else(|| Digest::of(b"undecided"))
}

fn launch_for(session_run: &mut Run<'_>, session: &SessionRef) -> Result<SessionLaunch, RunError> {
    let (listener, _stops) = FakeListener::bound();
    let key = ServingKey::new(vec![7u8; 32], Digest::of(b"session pin")).expect("key");
    Ok(SessionLaunch::launch(
        session_run,
        session,
        "supervisor-1",
        key,
        Box::new(listener),
    )?)
}

/// The receipt an activation publishes: its own plan, the run's publication and
/// the session's launch, which is what supplies the locator.
fn commit_receipt(
    cx: &EffectContext,
    root: &StateRoot,
    reservation: &WorkspaceActivationReservation,
    environment_run: &Run<'_>,
    session_run: &mut Run<'_>,
) -> Result<(SelectionReceipt, SessionLaunch), RunError> {
    let plan = plan_of(reservation, &read_journal(cx, root))
        .ok_or_else(|| RunError::refused("no staged plan to commit"))?;
    let publication = publication_for(cx, root, reservation, environment_run);
    let binary = Digest::parse_hex(&publication.publication.artifact_digest)
        .ok_or_else(|| RunError::refused("the publication names no artifact digest"))?;
    let selection_digest = reservation
        .inputs()
        .map(|inputs| inputs.selection.clone())
        .unwrap_or_else(|| Digest::of(b"undecided"));
    let mut launch = launch_for(session_run, &plan.session())?;
    let receipt = selection_receipt(
        &plan,
        publication,
        binary,
        None,
        &selection_digest,
        session_run,
        &mut launch,
    );
    Ok((receipt, launch))
}

fn commit_activation(
    cx: &EffectContext,
    root: &StateRoot,
    reservation: &WorkspaceActivationReservation,
    environment_run: &Run<'_>,
    session_run: &mut Run<'_>,
) -> Result<CommittedActivation, RunError> {
    let (receipt, launch) = commit_receipt(cx, root, reservation, environment_run, session_run)?;
    reservation.commit_selection(cx, root, environment_run, session_run, &receipt, &launch)
}

/// Commit *and* read the switch back: the generation is published and its
/// selection finished, exactly as a completed activation leaves it.
fn commit_and_switch(
    cx: &EffectContext,
    root: &StateRoot,
    reservation: &WorkspaceActivationReservation,
    environment_run: &mut Run<'_>,
    session_run: &mut Run<'_>,
) -> Result<CommittedActivation, RunError> {
    let (receipt, launch) = commit_receipt(cx, root, reservation, environment_run, session_run)?;
    let committed =
        reservation.commit_selection(cx, root, environment_run, session_run, &receipt, &launch)?;
    if let Some(obligation) = committed.selection_obligation() {
        let mut reader = FakeSelectors::exact(&receipt);
        Attachments::selection_switched(environment_run, obligation, &launch, &mut reader)?;
    }
    Ok(committed)
}

/// The validated selection a workspace activation publishes. The context
/// document is hashed here exactly as `prepare` would hash it, and no selector
/// is touched.
fn selection_receipt(
    plan: &ActivationPlan,
    publication: WorkspacePublicationReceipt,
    binary: Digest,
    previous: Option<&str>,
    selection_digest: &Digest,
    session_run: &Run<'_>,
    launch: &mut SessionLaunch,
) -> SelectionReceipt {
    // The document never names its own digest, so it is built from a
    // placeholder context object and then hashed: the real digest becomes the
    // context object the selection names, and the bytes are unchanged.
    let credential = Digest::of(b"enrol");
    let object = format!("/run/effigy/objects/bin/{binary}");
    let generation = Generation::new(publication.publication.context_generation);
    let context = Digest::of(&[]);
    let mut selection = GuestSelection {
        backend: "docker-desktop".to_owned(),
        objects: SelectionObjects {
            binary: GuestObject {
                path: object.clone(),
                digest: binary,
            },
            context: GuestObject {
                path: format!("/run/effigy/objects/context/{context}"),
                digest: context,
            },
            credential: GuestObject {
                path: format!("/run/effigy/objects/enrol/{}", generation.get()),
                digest: credential,
            },
            context_generation: generation,
            credential_generation: generation,
        },
        switches: [
            SelectorChange {
                path: BINARY_SELECTOR.to_owned(),
                expected: previous.map(|value| value.to_owned()),
                next: object.clone(),
            },
            SelectorChange {
                path: ENTRY_SELECTOR.to_owned(),
                expected: previous.map(|value| value.to_owned()),
                next: object.clone(),
            },
            SelectorChange {
                path: CONTEXT_SELECTOR.to_owned(),
                expected: previous.map(|value| value.to_owned()),
                next: generation.get().to_string(),
            },
        ],
    };
    launch
        .qualify(&mut verifying_probe(), Duration::from_secs(1))
        .expect("qualified");
    let lease = launch
        .publish(session_run)
        .expect("published")
        .lease()
        .clone();
    let document = context_document(plan, &publication, &selection, &lease, selection_digest);
    let context = Digest::of(&document);
    selection.objects.context = GuestObject {
        path: format!("/run/effigy/objects/context/{context}"),
        digest: context,
    };
    let mut prior = FakeSelectors::prior(&selection);
    SelectionReceipt::new(
        plan,
        publication,
        selection,
        &document,
        session_run,
        launch,
        &mut prior,
    )
    .expect("selection receipt")
}

/// One whole decision-driven activation: decide, stage, admit, publish, commit
/// with its selection and record the switch.
struct ActivatedSelection<'a> {
    session_run: Run<'a>,
    session: SessionRef,
    obligation: SelectionObligation,
}

/// End the environment activation run the way its own `up` does, so the
/// environment's `Gate(Env)` is free for the next decision.
fn finish_activation(cx: &EffectContext, run: Run<'_>) {
    end_run(
        cx,
        run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

fn activate_with_selection<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    environment: &ResourceId,
    session_id: RunId,
    activation_id: RunId,
    inputs: &PublishInputs,
) -> ActivatedSelection<'a> {
    let reservation =
        match WorkspaceActivationReservation::decide(cx, root, environment, inputs, Wait::FailFast)
            .expect("activation decision")
        {
            ActivationDecision::Activate(reservation) => *reservation,
            ActivationDecision::Attach(_) => panic!("a fresh environment cannot attach"),
        };
    let binary = inputs
        .artifact
        .selected()
        .cloned()
        .expect("a selected artifact");
    let (mut session_run, session) = admit_staged(cx, root, &reservation, session_id);
    let (environment_run, publication) =
        admit_activation_publishing(cx, root, &reservation, activation_id, &binary);
    // The plan is read once the activation run claimed E, so the publication
    // receipt is validated against the run that actually recorded it.
    let plan = plan_of(&reservation, &read_journal(cx, root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let receipt = selection_receipt(
        &plan,
        publication,
        binary,
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let committed = reservation
        .commit_selection(cx, root, &environment_run, &session_run, &receipt, &launch)
        .expect("selection commit");
    let obligation = committed
        .selection_obligation()
        .expect("the commit records its selection")
        .clone();
    let mut environment_run = environment_run;
    let mut reader = FakeSelectors::exact(&receipt);
    Attachments::selection_switched(&mut environment_run, &obligation, &launch, &mut reader)
        .expect("the switch is recorded");
    let switched = super::selection::read_obligation(
        read_journal(cx, root).conn(),
        &obligation.environment().clone(),
        obligation.environment_generation(),
    )
    .expect("read the obligation back")
    .expect("the obligation survives its switch");
    // The activation is complete: its own run finishes and releases `Gate(Env)`.
    finish_activation(cx, environment_run);
    ActivatedSelection {
        session_run,
        session,
        obligation: switched,
    }
}

fn obligation_row(
    reader: &StoreReader,
    environment: &ResourceId,
) -> Option<(String, String, String)> {
    reader
        .conn()
        .query_row(
            "SELECT state, run_id, session_run FROM workspace_selection_obligations WHERE environment=?1 \
             ORDER BY environment_generation DESC LIMIT 1",
            [environment.name().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .expect("obligation row")
}

fn reservation_rows(reader: &StoreReader, environment: &ResourceId) -> i64 {
    reader
        .conn()
        .query_row(
            "SELECT count(*) FROM workspace_activation_reservations WHERE environment=?1",
            [environment.name().to_string()],
            |row| row.get(0),
        )
        .expect("reservation rows")
}

#[test]
fn a_decision_attaches_the_live_session_that_serves_its_exact_inputs() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let inputs = publish_inputs(7, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(200), run_id(201), &inputs);
    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(activated.session.clone())
    );
    let generations = reservation_rows(&reader, &environment);

    // The same inputs, re-decided under the same gate: attach, change nothing.
    let decision = WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &publish_inputs(7, Some(published_target())),
        Wait::FailFast,
    )
    .expect("attach decision");
    match decision {
        ActivationDecision::Attach(session) => assert_eq!(session, activated.session),
        ActivationDecision::Activate(_) => panic!("identical inputs must attach"),
    }
    let reader = read_journal(&cx, &root);
    assert_eq!(
        reservation_rows(&reader, &environment),
        generations,
        "an attach reserves no generation"
    );
    assert_eq!(
        resource_row(&reader, &environment),
        (
            activated.obligation.environment_generation().get() as i64,
            Some("active".into()),
            Some(
                activated
                    .obligation
                    .environment_generation()
                    .get()
                    .to_string()
            )
        ),
        "the environment's generation did not advance"
    );
    assert!(is_live(&activated.session_run));
    assert_eq!(stop_requests(&cx, &root, activated.session.run()), 0);
    assert_eq!(activated.obligation.state(), SelectionState::Switched);
    end_run(
        &cx,
        activated.session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn competing_decisions_reserve_one_successor_generation_and_keep_the_predecessor() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let first = publish_inputs(11, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(202), run_id(203), &first);

    // Two callers that both see a changed artifact decide at the same time.
    let changed = publish_inputs(12, None);
    let one = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &changed,
        Wait::FailFast,
    )
    .expect("first decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("changed inputs cannot attach"),
    };
    let two = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &changed,
        Wait::FailFast,
    )
    .expect("competing decision recovers, never duplicates")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("changed inputs cannot attach"),
    };
    assert_eq!(one.id(), two.id(), "one successor generation per decision");
    assert_eq!(
        one.generation(),
        activated
            .obligation
            .environment_generation()
            .next()
            .expect("E+1"),
        "the successor is exactly the generation after the live one"
    );
    assert_eq!(one.inputs(), Some(&changed));

    // A third caller with yet other inputs meets the reserved fence by name.
    let error = WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &publish_inputs(13, None),
        Wait::FailFast,
    )
    .expect_err("a different intent cannot share the reserved generation");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");

    // The predecessor is untouched by any of it.
    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(activated.session.clone())
    );
    assert_eq!(
        attachment_row(&reader, &environment, activated.session.run()),
        Some((
            activated.obligation.environment_generation().get() as i64,
            "\"Borrow\"".to_owned(),
            "attached".to_owned()
        ))
    );
    assert!(is_live(&activated.session_run));
    assert_eq!(stop_requests(&cx, &root, activated.session.run()), 0);
    one.cancel(&cx, &root, Wait::FailFast)
        .expect("release the unused successor");
    end_run(
        &cx,
        activated.session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_decision_refuses_a_target_the_live_publication_does_not_name() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let inputs = publish_inputs(21, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(204), run_id(205), &inputs);
    let other = ContainerIdentity {
        container_id: "c3".repeat(32),
        vm_boot_id: published_target().vm_boot_id,
        backend: published_target().backend,
    };
    // The artifact changed too, so a comparison that looked at the artifact
    // first would quietly reserve a successor; the target fence comes first.
    let mut changed = publish_inputs(22, Some(other));
    changed.artifact = ArtifactEvidence::Selected {
        digest: artifact(23),
        object: format!("/run/effigy/objects/bin/{}", artifact(23)),
    };
    let error =
        WorkspaceActivationReservation::decide(&cx, &root, &environment, &changed, Wait::FailFast)
            .expect_err("a replaced container is refused, never superseded silently");
    assert!(
        matches!(error, RunError::Refused(ref message) if message.contains("is not the container and boot")),
        "{error:?}"
    );
    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(activated.session.clone())
    );
    assert_eq!(reservation_rows(&reader, &environment), 1);
    end_run(
        &cx,
        activated.session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_pending_selection_refuses_attach_and_reservation() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let inputs = publish_inputs(31, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let binary = artifact(31);
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(206));
    let (environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(207), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let receipt = selection_receipt(
        &plan,
        publication,
        binary,
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect("commit a Pending selection");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned())
    );
    // The activation run ends without switching anything: `Gate(Env)` is free
    // and the committed obligation is still Pending.
    finish_activation(&cx, environment_run);

    // Nothing attaches to a generation whose selection is unfinished, and
    // nothing reserves around it either.
    for decision in [
        WorkspaceActivationReservation::decide(&cx, &root, &environment, &inputs, Wait::FailFast),
        WorkspaceActivationReservation::decide(
            &cx,
            &root,
            &environment,
            &publish_inputs(32, None),
            Wait::FailFast,
        ),
    ] {
        let error = decision.expect_err("an unfinished selection is not bypassable");
        assert!(
            matches!(error, RunError::Refused(ref message) if message.contains("still unfinished")),
            "{error:?}"
        );
    }
    // The committed successor is left Pending by design; its launch custody is
    // released when the fixture goes away, which is what a crashed activation
    // leaves behind.
    drop(session_run);
}

// ---- 052 §8.11: the durable selection, its switch and its recovery ---------

#[test]
fn the_selection_commit_writes_mapping_borrow_and_pending_selection_together() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(41, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(208), run_id(209), &inputs);
    let reader = read_journal(&cx, &root);
    assert_eq!(
        resolved(&reader, &environment),
        Some(activated.session.clone()),
        "the mapping names the committed successor"
    );
    assert_eq!(
        attachment_row(&reader, &environment, activated.session.run()),
        Some((2, "\"Borrow\"".to_owned(), "attached".to_owned()))
    );
    assert_eq!(
        resource_row(&reader, &environment),
        (2, Some("active".into()), Some("2".into()))
    );
    let row = obligation_row(&reader, &environment).expect("obligation");
    assert_eq!(row.0, "switched", "the switch was recorded, not guessed");
    assert_eq!(
        row.1,
        activated
            .obligation
            .receipt()
            .publication()
            .activation_run
            .to_string()
    );
    assert_eq!(row.2, activated.session_run.id().to_string());
    assert_eq!(
        activated
            .obligation
            .receipt()
            .selection()
            .objects
            .context_generation,
        Generation::new(1)
    );
    assert_eq!(
        activated
            .obligation
            .receipt()
            .publication()
            .publication
            .artifact_digest,
        format!("sha256:{}", artifact(41).hex())
    );
    assert!(
        una_finished(&reader, &environment).is_none(),
        "a switched selection is finished"
    );
    end_run(
        &cx,
        activated.session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

/// `unfinished_selection`, as a plain option for assertions.
fn una_finished(reader: &StoreReader, environment: &ResourceId) -> Option<SelectionObligation> {
    match unfinished_selection(reader, environment) {
        Observed::Verified(obligation, _) => obligation,
        Observed::Absent(_) => None,
        Observed::Unknown(reason) => panic!("selection is unreadable: {reason}"),
    }
}

/// Commit a generation's selection and return the live handles, leaving the
/// obligation `Pending` exactly as a crash between commit and switch does.
fn commit_pending_selection<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    environment: &ResourceId,
    session_id: RunId,
    activation_id: RunId,
    inputs: &PublishInputs,
) -> (
    WorkspaceActivationReservation,
    Run<'a>,
    SessionRef,
    Run<'a>,
    SelectionReceipt,
    SelectionObligation,
    SessionLaunch,
) {
    let reservation =
        match WorkspaceActivationReservation::decide(cx, root, environment, inputs, Wait::FailFast)
            .expect("decision")
        {
            ActivationDecision::Activate(reservation) => *reservation,
            ActivationDecision::Attach(_) => panic!("fresh environment"),
        };
    let binary = inputs.artifact.selected().cloned().expect("selected");
    let (mut session_run, session) = admit_staged(cx, root, &reservation, session_id);
    let (environment_run, publication) =
        admit_activation_publishing(cx, root, &reservation, activation_id, &binary);
    let plan = plan_of(&reservation, &read_journal(cx, root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let receipt = selection_receipt(
        &plan,
        publication,
        binary,
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let committed = reservation
        .commit_selection(cx, root, &environment_run, &session_run, &receipt, &launch)
        .expect("selection commit");
    let obligation = committed
        .selection_obligation()
        .expect("selection obligation")
        .clone();
    (
        reservation,
        session_run,
        session,
        environment_run,
        receipt,
        obligation,
        launch,
    )
}

#[test]
fn a_switch_refuses_a_third_value_and_completes_once() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (_, session_run, _session, mut environment_run, receipt, obligation, launch) =
        commit_pending_selection(
            &cx,
            &root,
            &environment,
            run_id(210),
            run_id(211),
            &publish_inputs(42, None),
        );
    // A value that is not the obligation's target is refused, never recorded.
    let mut third = FakeSelectors::third(&receipt);
    assert!(
        Attachments::selection_switched(&mut environment_run, &obligation, &launch, &mut third)
            .is_err(),
        "a third value is not a completion"
    );
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned()),
        "a refused switch leaves the obligation pending"
    );

    // The exact target completes it exactly once.
    Attachments::selection_switched(
        &mut environment_run,
        &obligation,
        &launch,
        &mut FakeSelectors::exact(&receipt),
    )
    .expect("exact switch");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("switched".to_owned())
    );
    assert!(una_finished(&reader, &environment).is_none());
    let error = Attachments::selection_switched(
        &mut environment_run,
        &obligation,
        &launch,
        &mut FakeSelectors::exact(&receipt),
    )
    .expect_err("a selection completes once");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_crashed_switch_is_recovered_from_the_durable_obligation() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (reservation, session_run, session, environment_run, receipt, obligation, launch) =
        commit_pending_selection(
            &cx,
            &root,
            &environment,
            run_id(212),
            run_id(213),
            &publish_inputs(43, None),
        );
    // The activation process is gone: its own handles prove nothing.
    drop(environment_run);
    let reader = read_journal(&cx, &root);
    let pending = una_finished(&reader, &environment).expect("the obligation survives the crash");
    assert_eq!(pending.id(), obligation.id());
    assert_eq!(pending.reservation(), reservation.id());
    assert_eq!(pending.environment_generation(), Generation::new(2));
    assert_eq!(pending.session_run(), session.run());
    let plan = plan_of(&reservation, &reader).expect("the committed plan");
    assert_eq!(
        pending.receipt().publication(),
        receipt.publication(),
        "the receipt is the exact committed one"
    );
    assert_eq!(plan.session(), session);

    // Completion needs all three facts. A dead listener refuses completion...
    let live = SelectionLiveness {
        session: Observed::Verified((), evidence()),
        target: Observed::Verified((), evidence()),
        listener: Observed::Verified((), evidence()),
    };
    let error = selection_recovered(
        &cx,
        &root,
        &pending,
        &launch,
        &mut verifying_target(),
        &mut FakeProbe {
            answer: Some(Observed::Absent(crate::observe::AbsenceProof::NoRow {
                table: "workspace-session-listener".into(),
            })),
            calls: 0,
        },
        &mut FakeSelectors::exact(&receipt),
        Wait::FailFast,
    )
    .expect_err("a dead listener cannot complete a selection");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    // Nor can a live listener complete a selection whose readback is not the
    // committed target: recovery repeats the same compares, it does not assume
    // they happened.
    let error = selection_recovered(
        &cx,
        &root,
        &pending,
        &launch,
        &mut verifying_target(),
        &mut verifying_probe(),
        &mut FakeSelectors::third(&receipt),
        Wait::FailFast,
    )
    .expect_err("a third value is never a completed selection");
    // A missing object is not a completed selection either.
    assert!(
        selection_recovered(
            &cx,
            &root,
            &pending,
            &launch,
            &mut verifying_target(),
            &mut verifying_probe(),
            &mut FakeSelectors::missing_object(&receipt),
            Wait::FailFast,
        )
        .is_err(),
        "a missing object is never a completed selection"
    );
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    // ...and it cannot be abandoned while completion is still possible.
    let error = selection_abandoned(
        &cx,
        &root,
        &pending,
        &live,
        "would strand a live generation",
        Wait::FailFast,
    )
    .expect_err("a completable selection is not abandoned");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");

    // The three proven facts plus the exact readback complete it, without the
    // dead activation run.
    selection_recovered(
        &cx,
        &root,
        &pending,
        &launch,
        &mut verifying_target(),
        &mut verifying_probe(),
        &mut FakeSelectors::exact(&receipt),
        Wait::FailFast,
    )
    .expect("recovery completes the exact committed selection");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("switched".to_owned())
    );
    assert!(una_finished(&reader, &environment).is_none());
    assert_eq!(resolved(&reader, &environment), Some(session));
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn an_established_dead_listener_abandons_and_revokes_without_restoring_the_predecessor() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let first = publish_inputs(51, None);
    let predecessor =
        activate_with_selection(&cx, &root, &environment, run_id(214), run_id(215), &first);
    let predecessor_session = predecessor.session.clone();
    let (_, session_run, session, environment_run, _, obligation, _launch) =
        commit_pending_selection(
            &cx,
            &root,
            &environment,
            run_id(216),
            run_id(217),
            &publish_inputs(52, None),
        );
    finish_activation(&cx, environment_run);
    let reader = read_journal(&cx, &root);
    assert_eq!(resolved(&reader, &environment), Some(session.clone()));
    assert_eq!(
        una_finished(&reader, &environment).map(|o| o.id()),
        Some(obligation.id())
    );

    // Unknown liveness grants nothing: it is neither completion nor abandonment.
    let unknown = SelectionLiveness {
        session: Observed::Unknown(UnknownReason::Unparsable("listener unreachable".into())),
        target: Observed::Verified((), evidence()),
        listener: Observed::Unknown(UnknownReason::Unparsable("listener unreachable".into())),
    };
    selection_abandoned(&cx, &root, &obligation, &unknown, "unknown", Wait::FailFast)
        .expect_err("unknown liveness stays pending");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned())
    );

    // A live successor with an established dead listener is not abandoned
    // around: the exact run is stopped and settled under its own private slot
    // first, so nothing is left holding a task slot, a listener or a holder.
    let dead_while_live = SelectionLiveness {
        session: Observed::Verified((), evidence()),
        target: Observed::Verified((), evidence()),
        listener: Observed::Absent(crate::observe::AbsenceProof::NoRow {
            table: "workspace-session-listener".into(),
        }),
    };
    let error = selection_abandoned(
        &cx,
        &root,
        &obligation,
        &dead_while_live,
        "listener established dead",
        Wait::FailFast,
    )
    .expect_err("a running successor is settled before its generation is revoked");
    assert!(
        matches!(error, RunError::Refused(ref message) if message.contains("stop and settle it")),
        "{error:?}"
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    // Settled successor, dead listener: abandon, not refuse.
    let dead = SelectionLiveness {
        session: Observed::Absent(crate::observe::AbsenceProof::NoRow {
            table: "runs".into(),
        }),
        target: Observed::Verified((), evidence()),
        listener: Observed::Absent(crate::observe::AbsenceProof::NoRow {
            table: "workspace-session-listener".into(),
        }),
    };
    selection_abandoned(
        &cx,
        &root,
        &obligation,
        &dead,
        "listener established dead",
        Wait::FailFast,
    )
    .expect("an unserved generation is abandoned");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment),
        Some((
            "abandoned".to_owned(),
            obligation.run().to_string(),
            session.run().to_string()
        ))
    );
    assert_eq!(
        mapping(&reader, &environment),
        None,
        "the revoked generation publishes nothing"
    );
    assert_eq!(
        resolved(&reader, &environment),
        None,
        "the predecessor's authority is not restored"
    );
    assert_eq!(
        resource_row(&reader, &environment),
        (2, Some("pending".into()), None),
        "the environment no longer advertises generation 2"
    );

    // The next decision reserves the next generation; the abandoned one is not
    // revived and no predecessor is re-published.
    let next = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &publish_inputs(53, None),
        Wait::FailFast,
    )
    .expect("decision after abandonment")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("a revoked generation does not attach"),
    };
    assert_eq!(next.generation(), Generation::new(3));
    next.cancel(&cx, &root, Wait::FailFast)
        .expect("release the unused successor");
    assert_ne!(session, predecessor_session);
    end_run(
        &cx,
        predecessor.session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn the_generic_commit_cannot_publish_a_generation_without_its_selection() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(222));
    // Everything the old generic commit needed is present, including a complete
    // applied publication: the refusal is about the missing selection record,
    // not about the run.
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(223));
    let error = reservation
        .commit(&cx, &root, &environment_run, &session_run)
        .expect_err("the generic commit cannot publish without a selection");
    assert!(
        matches!(error, RunError::Refused(ref message) if message.contains("use commit_selection")),
        "{error:?}"
    );
    let reader = read_journal(&cx, &root);
    assert_eq!(
        mapping(&reader, &environment),
        None,
        "nothing is published by the generic commit"
    );
    assert_eq!(obligation_row(&reader, &environment), None);
    assert_eq!(
        attachment_row(&reader, &environment, session_run.id()),
        None
    );
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_selection_never_names_another_session_s_listener() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(64, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let binary = artifact(64);
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(224));
    let (environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(225), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    // A session ref another run claims has no owner proof for this run, so no
    // launch exists to publish its locator at all.
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let selection = selection_receipt(
        &plan,
        publication.clone(),
        binary.clone(),
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    )
    .selection()
    .clone();
    let elsewhere = SessionRef::new(
        run_id(226),
        plan.session().fence().clone(),
        plan.session().environment().clone(),
        plan.session().environment_generation(),
    );
    let (listener, _stops) = FakeListener::bound();
    let key = ServingKey::new(vec![1u8; 32], Digest::of(b"session pin")).expect("key");
    assert!(matches!(
        SessionLaunch::launch(
            &mut session_run,
            &elsewhere,
            "supervisor-1",
            key,
            Box::new(listener)
        ),
        Err(ServingRefusal::WrongSession)
    ));
    // A listener that never answered a qualification cannot publish a locator,
    // so it cannot author a receipt either.
    let (listener, _stops) = FakeListener::bound();
    let key = ServingKey::new(vec![1u8; 32], Digest::of(b"session pin")).expect("key");
    let mut unqualified = SessionLaunch::launch(
        &mut session_run,
        &plan.session(),
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    let mut published = launch_for(&mut session_run, &plan.session()).expect("custody");
    let document = {
        published
            .qualify(&mut verifying_probe(), Duration::from_secs(1))
            .expect("qualified");
        let lease = published
            .publish(&session_run)
            .expect("published")
            .lease()
            .clone();
        context_document(
            &plan,
            &publication,
            &selection,
            &lease,
            &receipt_selection(&reservation),
        )
    };
    let error = SelectionReceipt::new(
        &plan,
        publication,
        selection,
        &document,
        &session_run,
        &mut unqualified,
        &mut FakeSelectors::default(),
    )
    .expect_err("an unqualified listener publishes nothing");
    assert!(
        matches!(error, RunError::Refused(ref message) if message.contains("qualified")),
        "{error:?}"
    );
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_selection_commit_is_idempotent_and_refuses_other_bytes() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(61, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let binary = artifact(61);
    let (mut session_run, session) = admit_staged(&cx, &root, &reservation, run_id(218));
    let (environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(219), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let receipt = selection_receipt(
        &plan,
        publication.clone(),
        binary.clone(),
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let committed = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect("selection commit");
    let obligation = committed
        .selection_obligation()
        .expect("obligation")
        .clone();
    let again = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect("a retried commit answers with the same obligation");
    assert_eq!(
        again.selection_obligation().map(SelectionObligation::id),
        Some(obligation.id())
    );
    // Different bytes for an already committed generation are refused.
    // A different selection for the same publication: another expected-previous
    // value is another selection, and the committed generation is frozen.
    let other = selection_receipt(
        &plan,
        publication,
        binary,
        Some("an earlier value"),
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let error = reservation
        .commit_selection(&cx, &root, &environment_run, &session_run, &other, &launch)
        .expect_err("a committed selection is frozen");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned()),
        "no second obligation was written"
    );
    assert_eq!(resolved(&reader, &environment), Some(session));
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_selection_receipt_refuses_a_document_that_is_not_the_committed_context() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let reservation =
        WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
            .expect("reservation");
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(220));
    let binary = artifact(63);
    let (_, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(221), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let good = selection_receipt(
        &plan,
        publication.clone(),
        binary,
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    assert!(good.validate(&plan).is_ok());
    // A selection whose context object names other bytes is not the receipt.
    let mut wrong = good.selection().clone();
    wrong.objects.context.digest = Digest::of(b"not the document");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    assert!(
        SelectionReceipt::new(
            &plan,
            publication,
            wrong,
            b"the document",
            &session_run,
            &mut launch,
            &mut FakeSelectors::default(),
        )
        .is_err(),
        "the document digest is hashed, never taken on trust"
    );
}

// ---- 052 §8.11: the session-owned listener and key ------------------------

#[derive(Debug)]
struct FakeListener {
    endpoint: Option<std::net::SocketAddr>,
    phase: ListenerPhase,
    stops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl FakeListener {
    fn bound() -> (Self, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        let stops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        (
            Self {
                endpoint: Some("127.0.0.1:45999".parse().expect("address")),
                phase: ListenerPhase::Qualifying,
                stops: stops.clone(),
            },
            stops,
        )
    }
}

impl ChannelListener for FakeListener {
    fn endpoint(&self) -> Option<std::net::SocketAddr> {
        self.endpoint
    }
    fn phase(&self) -> ListenerPhase {
        self.phase
    }
    fn qualify(&mut self) -> Result<(), ServingRefusal> {
        if self.phase == ListenerPhase::Stopped {
            return Err(ServingRefusal::Stopped);
        }
        self.phase = ListenerPhase::Holding;
        Ok(())
    }
    fn stop(&mut self) {
        self.stops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.phase = ListenerPhase::Stopped;
    }
}

#[derive(Debug, Default)]
struct FakeProbe {
    answer: Option<Observed<()>>,
    calls: usize,
}

impl ServingProbe for FakeProbe {
    fn pinned_handshake(
        &mut self,
        _endpoint: std::net::SocketAddr,
        _pin: &Digest,
        _deadline: Duration,
    ) -> Observed<()> {
        self.calls += 1;
        self.answer.clone().unwrap_or_else(|| {
            Observed::Unknown(UnknownReason::Unparsable("no probe answer".into()))
        })
    }
}

/// The lease-facing identity of an admitted session.
fn session_binding(session: &SessionRef) -> SessionBinding {
    SessionBinding {
        owner: session.fence().resource.clone(),
        run: session.run(),
        generation: session.generation(),
    }
}

/// A fixture with one admitted, detached workspace session, whose run owns the
/// listener and key custody these tests exercise.
struct ServingFixture {
    cx: EffectContext,
    #[allow(dead_code)]
    home: effigy_test_support::Fixture,
    root: StateRoot,
    environment: ResourceId,
}

impl ServingFixture {
    fn new() -> Self {
        let cx = context();
        let home = home();
        let root = StateRoot::effects(&cx, home.path()).expect("state root");
        let (environment, _fence) = active_environment(&cx, &root);
        Self {
            cx,
            home,
            root,
            environment,
        }
    }
    fn admit(&self, id: u8) -> (Run<'_>, SessionRef) {
        let fence = Fence {
            resource: self.environment.clone(),
            generation: Generation::new(1),
        };
        let mut options = opts(run_id(id));
        options.id = run_id(id);
        let (mut run, session) = SessionRef::admit(
            &self.cx,
            &self.root,
            &inv(workspace_entry(), "workspace"),
            options,
            &cleanup(&self.cx),
            &mut adapter(),
            &fence,
            Placement::Detached,
        )
        .expect("workspace session");
        run.register_session_custody("supervisor-1")
            .expect("detached custody");
        (run, session)
    }
}

#[test]
fn custody_survives_the_caller_and_publishes_only_a_bound_qualified_listener() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(70);
    let (listener, stops) = FakeListener::bound();
    let key = ServingKey::new(vec![7u8; 32], Digest::of(b"session pin")).expect("key");
    let mut launch = SessionLaunch::launch(
        &mut session_run,
        &session,
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    // The supervisor holds both the run and the launch: the caller that asked
    // for the session is gone, and the launch keeps its listener and key.
    assert_eq!(launch.phase(), ListenerPhase::Qualifying);
    assert_eq!(stops.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        matches!(
            launch.publish(&session_run),
            Err(ServingRefusal::Unqualified)
        ),
        "an unqualified listener is never published"
    );
    // The transport qualifies the endpoint; the launch owns the fact.
    launch
        .qualify(
            &mut FakeProbe {
                answer: Some(Observed::Verified((), evidence())),
                calls: 0,
            },
            Duration::from_secs(1),
        )
        .expect("qualified");
    let lease = launch
        .publish(&session_run)
        .expect("a bound, qualified listener publishes")
        .lease()
        .clone();
    assert_eq!(lease.endpoint(), "127.0.0.1:45999");
    assert_eq!(lease.pin(), &Digest::of(b"session pin"));
    assert_eq!(lease.supervisor_id(), "supervisor-1");
    assert_eq!(lease.session(), &session_binding(&session));
    assert_eq!(
        launch
            .publish(&session_run)
            .expect("publish is idempotent")
            .lease(),
        &lease,
        "one session has one locator"
    );
    // A retried publish still answers the same lease: retrying never rebinds.
    assert_eq!(
        launch
            .publish(&session_run)
            .expect("retried publish")
            .lease(),
        &lease,
        "a retry does not re-lease the session"
    );
    assert_eq!(stops.load(std::sync::atomic::Ordering::SeqCst), 0);
    let probed = launch.liveness(
        &session_run,
        &mut FakeProbe {
            answer: Some(Observed::Verified((), evidence())),
            calls: 0,
        },
        Duration::from_secs(1),
    );
    assert!(
        probed.verified().is_some(),
        "the held key answers a bounded pinned handshake"
    );
    // Only an adopted session serves the channel.
    launch.serve(&session_run).expect("adopted");
    assert!(launch.serving(&session_run));
    launch.release();
    assert_eq!(
        stops.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "release stops the listener"
    );
}

#[test]
fn a_listener_is_only_launched_for_the_run_that_owns_the_session() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(75);
    let _owner = session_run
        .session_owner()
        .expect("the session's owning run");
    // A session ref another run claims is not the one this owner holds.
    let elsewhere = SessionRef::new(
        run_id(76),
        session.fence().clone(),
        session.environment().clone(),
        session.environment_generation(),
    );
    let (listener, stops) = FakeListener::bound();
    let key = ServingKey::new(vec![5u8; 32], Digest::of(b"pin")).expect("key");
    assert!(matches!(
        SessionLaunch::launch(
            &mut session_run,
            &elsewhere,
            "supervisor-1",
            key,
            Box::new(listener)
        ),
        Err(ServingRefusal::WrongSession)
    ));
    assert_eq!(
        stops.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a refused launch stops nothing"
    );
    // An environment run holds no session custody at all.
    let mut environment_run =
        admit_activation_stub(&fixture.cx, &fixture.root, &fixture.environment);
    assert!(environment_run.session_owner().is_err());
}

/// A detached environment run, for the custody proof above: it owns an
/// environment, never a workspace session.
fn admit_activation_stub<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    environment: &ResourceId,
) -> Run<'a> {
    Run::begin(
        cx,
        root,
        &inv(environment.clone(), "environment:up"),
        opts(run_id(77)),
        &cleanup(cx),
        &mut adapter(),
    )
    .expect("environment run")
}

#[test]
fn a_stopped_or_unbound_listener_never_yields_channel_bound() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(71);
    let _owner = session_run.session_owner().expect("owner");
    // No endpoint at all is not a listener.
    let stops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let unbound = FakeListener {
        endpoint: None,
        phase: ListenerPhase::Qualifying,
        stops: stops.clone(),
    };
    let key = ServingKey::new(vec![1u8; 32], Digest::of(b"pin")).expect("key");
    assert!(matches!(
        SessionLaunch::launch(
            &mut session_run,
            &session,
            "supervisor-1",
            key,
            Box::new(unbound)
        ),
        Err(ServingRefusal::Unbound)
    ));
    // A stopped listener is never adopted into a launch, so it can never be
    // published under a locator that no longer answers.
    let stopped = FakeListener {
        endpoint: Some("127.0.0.1:45998".parse().expect("address")),
        phase: ListenerPhase::Stopped,
        stops,
    };
    let key = ServingKey::new(vec![1u8; 32], Digest::of(b"pin")).expect("key");
    assert!(matches!(
        SessionLaunch::launch(
            &mut session_run,
            &session,
            "supervisor-1",
            key,
            Box::new(stopped)
        ),
        Err(ServingRefusal::Stopped)
    ));
    // And one stopped after publication refuses both publish and serve.
    let (listener, _stops) = FakeListener::bound();
    let key = ServingKey::new(vec![2u8; 32], Digest::of(b"pin")).expect("key");
    let mut launch = SessionLaunch::launch(
        &mut session_run,
        &session,
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    launch
        .qualify(
            &mut FakeProbe {
                answer: Some(Observed::Verified((), evidence())),
                calls: 0,
            },
            Duration::from_secs(1),
        )
        .expect("qualified");
    launch
        .publish(&session_run)
        .expect("published before it stops");
    launch.stop();
    assert!(matches!(
        launch.publish(&session_run),
        Err(ServingRefusal::Stopped)
    ));
    assert!(matches!(
        launch.serve(&session_run),
        Err(ServingRefusal::Unqualified)
    ));
    let absent = launch.liveness(
        &session_run,
        &mut FakeProbe {
            answer: Some(Observed::Verified((), evidence())),
            calls: 0,
        },
        Duration::from_secs(1),
    );
    assert!(
        matches!(absent, Observed::Absent(_)),
        "a stopped listener has no liveness"
    );
}

#[test]
fn a_real_bound_socket_is_the_endpoint_the_lease_names() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(74);
    let listener =
        HostChannelListener::bind("127.0.0.1:0".parse().expect("address")).expect("bind");
    let endpoint = listener.endpoint().expect("bound endpoint");
    let key = ServingKey::new(vec![3u8; 32], Digest::of(b"real pin")).expect("key");
    let mut launch = SessionLaunch::launch(
        &mut session_run,
        &session,
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    launch
        .qualify(&mut verifying_probe(), Duration::from_secs(1))
        .expect("qualified");
    let lease = launch
        .publish(&session_run)
        .expect("publish")
        .lease()
        .clone();
    assert_eq!(lease.endpoint(), endpoint.to_string());
    // The published locator is reachable while the launch holds it.
    std::net::TcpStream::connect(endpoint).expect("the published endpoint accepts");
    launch.release();
    assert!(
        std::net::TcpStream::connect(endpoint).is_err(),
        "release closes the endpoint the lease named"
    );
}

#[test]
fn a_session_key_never_prints_its_secret() {
    let key = ServingKey::new(vec![9u8; 16], Digest::of(b"pin")).expect("key");
    let printed = format!("{key:?}");
    assert_eq!(printed, "ServingKey(<redacted>)");
    assert!(!printed.contains('9'));
    assert!(ServingKey::new(Vec::new(), Digest::of(b"pin")).is_err());
    assert!(ServingKey::new(vec![1u8; 8], Digest([0u8; 32])).is_err());
}

/// Fail each boundary of one decision-driven activation and lose both handles,
/// as a crash does. The mapping, the session's `Borrow` attachment, `active(E)`
/// and the `Pending` selection obligation are all committed or none is; a
/// committed generation whose switch never ran is recovered from its own
/// obligation, never from a guess.
#[cfg(feature = "fault-injection")]
#[test]
fn every_selection_boundary_keeps_the_generation_and_its_obligation_atomic() {
    use crate::fault::{arm, arm_nth, fired, Fault, Point};
    let boundaries = [
        (Point::ActivationDecisionBefore, false, None),
        (Point::ActivationDecisionAfter, false, None),
        (Point::SelectionCommitBefore, false, None),
        (Point::ActivationMappingBefore, false, None),
        (Point::ActivationAttachmentBefore, false, None),
        (Point::SelectionCommitAfter, true, Some("pending")),
        (Point::SelectionSwitchBefore, true, Some("pending")),
        (Point::SelectionSwitchAfter, true, Some("switched")),
    ];
    for (point, committed, state) in boundaries {
        let cx = context();
        let fixture = home();
        let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
        let environment = pending_environment(&cx, &root);
        let inputs = publish_inputs(70, None);
        let binary = artifact(70);
        let mut session_run = None;
        let mut activation_run = None;
        let mut obligation = None;
        let mut retained_launch = None;
        arm_nth(point, 0, Fault::Enospc);
        let result = (|| -> Result<(), RunError> {
            let reservation = match WorkspaceActivationReservation::decide(
                &cx,
                &root,
                &environment,
                &inputs,
                Wait::FailFast,
            )? {
                ActivationDecision::Activate(reservation) => *reservation,
                ActivationDecision::Attach(_) => {
                    return Err(RunError::broken("a fresh environment cannot attach"))
                }
            };
            let (staged, _) = admit_staged(&cx, &root, &reservation, run_id(230));
            session_run = Some(staged);
            let (activating, publication) =
                admit_activation_publishing(&cx, &root, &reservation, run_id(231), &binary);
            activation_run = Some(activating);
            let activating = activation_run.as_ref().expect("activation run");
            let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
            let staged = session_run.as_mut().expect("owning successor");
            let mut launch = launch_for(staged, &plan.session()).expect("session custody");
            let receipt = selection_receipt(
                &plan,
                publication,
                binary,
                None,
                &receipt_selection(&reservation),
                session_run.as_ref().expect("owning successor"),
                &mut launch,
            );
            retained_launch = Some(launch);
            let launch = retained_launch.as_ref().unwrap();
            let committed = reservation.commit_selection(
                &cx,
                &root,
                activating,
                session_run.as_ref().expect("owning successor"),
                &receipt,
                launch,
            )?;
            obligation = committed.selection_obligation().cloned();
            let activating = activation_run.as_mut().expect("activation run");
            Attachments::selection_switched(
                activating,
                obligation.as_ref().expect("selection obligation"),
                launch,
                &mut FakeSelectors::exact(&receipt),
            )?;
            Ok(())
        })();
        assert!(fired(), "fault {point:?} must fire");
        assert!(result.is_err(), "fault {point:?} must not report success");
        arm(None);
        drop(activation_run);
        drop(session_run);

        let reader = read_journal(&cx, &root);
        let generation = 1;
        if committed {
            assert_eq!(
                mapping(&reader, &environment).map(|row| row.0),
                Some(run_id(230).to_string()),
                "{point:?}: the mapping is committed with its obligation"
            );
            assert_eq!(
                attachment_row(&reader, &environment, run_id(230)),
                Some((generation, "\"Borrow\"".to_owned(), "attached".to_owned())),
                "{point:?}: the session borrowed the committed generation"
            );
            assert_eq!(
                resource_row(&reader, &environment),
                (
                    generation,
                    Some("active".into()),
                    Some(generation.to_string())
                ),
                "{point:?}: active(E) is the committed generation"
            );
        } else {
            assert_eq!(
                mapping(&reader, &environment),
                None,
                "{point:?}: no generation is published without its commit"
            );
            assert_ne!(
                resource_row(&reader, &environment).1.as_deref(),
                Some("active"),
                "{point:?}: an unfinished activation advertises nothing"
            );
            assert_eq!(
                obligation_row(&reader, &environment),
                None,
                "{point:?}: no obligation exists before the commit"
            );
        }
        if let Some(state) = state {
            assert_eq!(
                obligation_row(&reader, &environment).map(|row| row.0),
                Some(state.to_owned()),
                "{point:?}: the obligation records exactly what ran"
            );
        }
        if state == Some("pending") {
            // Both runs died, so the committed successor has no live owner:
            // recovery refuses to complete a selection nothing is serving, and
            // the obligation stays exactly where it was (recoverable, not lost).
            let pending = una_finished(&reader, &environment).expect("unfinished selection");
            let error = selection_recovered(
                &cx,
                &root,
                &pending,
                retained_launch.as_ref().unwrap(),
                &mut verifying_target(),
                &mut verifying_probe(),
                &mut FakeSelectors::exact(pending.receipt()),
                Wait::FailFast,
            )
            .expect_err("a dead successor is never completed");
            assert!(matches!(error, RunError::Refused(_)), "{error:?}");
            assert!(
                matches!(
                    WorkspaceActivationReservation::decide(
                        &cx,
                        &root,
                        &environment,
                        &decision_inputs(71),
                        Wait::FailFast
                    ),
                    Err(RunError::Refused(_))
                ),
                "dead Pending successor cannot be bypassed"
            );
            let reader = read_journal(&cx, &root);
            assert_eq!(
                obligation_row(&reader, &environment).map(|row| row.0),
                Some("pending".to_owned()),
                "{point:?}: the obligation is preserved for a live recovery"
            );
        }
        // No replay: settle the exact dead runs, then release an uncommitted
        // fence. Nothing about a failed activation is retried as a guest effect.
        for (owner, id) in [
            (environment.clone(), run_id(231)),
            (workspace_entry(), run_id(230)),
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
                .unwrap_or_else(|error| panic!("recover {id} after {point:?}: {error}"));
            }
        }
        if !committed {
            // A decision that never reserved leaves no fence to release; one
            // that did is recoverable and releasable, never hidden.
            if let Some(recovered) =
                WorkspaceActivationReservation::recover(&read_journal(&cx, &root), &environment)
                    .verified()
                    .cloned()
                    .flatten()
            {
                recovered
                    .cancel(&cx, &root, Wait::FailFast)
                    .expect("release the failed activation");
            }
        }
    }
}

// ---- 052 §8.11: concurrent decision, partial switch, frozen inputs ---------

/// Two callers that decide at the same time, on the same environment, through
/// the real gate and the real journal. The thread bodies are fakes; the
/// contention is real.
fn concurrent_decisions(
    home: std::path::PathBuf,
    inputs: PublishInputs,
    callers: usize,
) -> Vec<ActivationDecision> {
    let deadline = Instant::now() + Duration::from_secs(30);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(callers));
    let mut handles = Vec::new();
    for _ in 0..callers {
        let home = home.clone();
        let inputs = inputs.clone();
        let barrier = barrier.clone();
        handles.push(std::thread::spawn(move || {
            let cx = context();
            let root = StateRoot::effects(&cx, &home).expect("thread state root");
            let environment = environment();
            barrier.wait();
            WorkspaceActivationReservation::decide(
                &cx,
                &root,
                &environment,
                &inputs,
                crate::gate::Wait::Until(deadline),
            )
            .expect("concurrent decision")
        }));
    }
    handles
        .into_iter()
        .map(|handle| handle.join().expect("decision thread"))
        .collect()
}

#[test]
fn concurrent_decisions_reserve_one_generation_or_attach_one_session() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let live = publish_inputs(90, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(240), run_id(241), &live);
    let session = activated.session.clone();
    let generations = reservation_rows(&read_journal(&cx, &root), &environment);

    // Two callers with the live generation's exact inputs both attach, and
    // neither reserves anything.
    let attached = concurrent_decisions(fixture.path().to_path_buf(), live.clone(), 2);
    for decision in &attached {
        match decision {
            ActivationDecision::Attach(found) => assert_eq!(found, &session),
            ActivationDecision::Activate(_) => panic!("matching inputs must attach"),
        }
    }
    let reader = read_journal(&cx, &root);
    assert_eq!(
        reservation_rows(&reader, &environment),
        generations,
        "concurrent attaches reserve nothing"
    );

    // Two callers with changed inputs reserve one successor generation: the
    // loser recovers the winner's reservation instead of creating a second.
    let changed = publish_inputs(91, None);
    let successors = concurrent_decisions(fixture.path().to_path_buf(), changed.clone(), 2);
    let mut ids = Vec::new();
    for decision in successors {
        match decision {
            ActivationDecision::Activate(reservation) => {
                assert_eq!(reservation.inputs(), Some(&changed));
                ids.push((reservation.id(), reservation.generation()));
            }
            ActivationDecision::Attach(_) => panic!("changed inputs must reserve"),
        }
    }
    assert_eq!(ids[0], ids[1], "one successor generation for both callers");
    assert_eq!(ids[0].1, Generation::new(2));
    let reader = read_journal(&cx, &root);
    assert_eq!(
        reservation_rows(&reader, &environment),
        generations + 1,
        "two competing decisions write exactly one successor"
    );
    // The live generation is untouched by either decision.
    assert_eq!(resolved(&reader, &environment), Some(session));
    assert_eq!(stop_requests(&cx, &root, run_id(240)), 0);
    drop(activated);
}

#[test]
fn a_partial_switch_readback_is_refused_and_retried_to_exactness() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(92, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .unwrap()
    {
        ActivationDecision::Activate(r) => *r,
        _ => panic!("activate"),
    };
    let binary = inputs.artifact.selected().unwrap().clone();
    let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(242));
    let (mut environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(243), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).unwrap();
    let mut launch = launch_for(&mut session_run, &plan.session()).unwrap();
    let receipt = selection_receipt(
        &plan,
        publication,
        binary,
        Some("prior"),
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let mut partial = FakeSelectors::default();
    for object in [
        &receipt.selection().objects.binary,
        &receipt.selection().objects.context,
        &receipt.selection().objects.credential,
    ] {
        assert!(partial
            .objects
            .insert(
                object.path.clone(),
                Observed::Verified(object.digest.clone(), evidence())
            )
            .is_none());
    }
    for change in &receipt.selection().switches {
        partial.selectors.insert(
            change.path.clone(),
            Observed::Verified(change.expected.clone().unwrap(), evidence()),
        );
    }
    let staged = partial.objects.clone();
    assert!(una_finished(&read_journal(&cx, &root), &environment).is_none());
    let committed = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .unwrap();
    let obligation = committed.selection_obligation().unwrap();
    assert!(una_finished(&read_journal(&cx, &root), &environment).is_some());
    // Every switch compares the same mutable target with expected or next.
    let switch = |guest: &mut FakeSelectors, change: &SelectorChange| -> bool {
        let value = guest
            .selectors
            .get(&change.path)
            .unwrap()
            .verified()
            .unwrap();
        if value == &change.next {
            return true;
        }
        if Some(value) != change.expected.as_ref() {
            return false;
        }
        guest.selectors.insert(
            change.path.clone(),
            Observed::Verified(change.next.clone(), evidence()),
        );
        true
    };
    for change in &receipt.selection().switches[..2] {
        assert!(switch(&mut partial, change));
    }
    let error =
        Attachments::selection_switched(&mut environment_run, obligation, &launch, &mut partial)
            .expect_err("a partial switch is not a completion");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned()),
        "a partial switch leaves the obligation pending"
    );
    assert!(una_finished(&reader, &environment).is_some());

    let last = &receipt.selection().switches[2];
    partial.selectors.insert(
        last.path.clone(),
        Observed::Verified("unknown third".into(), evidence()),
    );
    assert!(!switch(&mut partial, last));
    assert_eq!(
        partial.selectors[&last.path].verified().unwrap(),
        "unknown third"
    );
    // External repair restores the known prior value; retry uses the same target.
    partial.selectors.insert(
        last.path.clone(),
        Observed::Verified(last.expected.clone().unwrap(), evidence()),
    );
    for change in &receipt.selection().switches {
        assert!(switch(&mut partial, change));
    }
    assert_eq!(
        partial.objects, staged,
        "immutable objects never change during switching"
    );
    Attachments::selection_switched(&mut environment_run, obligation, &launch, &mut partial)
        .expect("retry completes");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("switched".to_owned())
    );
    assert!(una_finished(&reader, &environment).is_none());
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_commit_refuses_an_artifact_the_decision_did_not_freeze() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let frozen = decision_inputs(93);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &frozen,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh generation"),
    };
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(244));
    // The activation publishes an artifact the decision never froze.
    let published = artifact(94);
    let (environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(245), &published);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("session custody");
    let receipt = selection_receipt(
        &plan,
        publication,
        published,
        None,
        &receipt_selection(&reservation),
        &session_run,
        &mut launch,
    );
    let error = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect_err("the frozen artifact is the one that must be published");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(obligation_row(&reader, &environment), None);
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    // The failed launch held the session's custody; release it before the dead
    // successor is settled under its own slot.
    drop(launch);
    drop(session_run);
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(244),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("settle the failed successor");
}

#[test]
fn a_commit_without_live_session_custody_publishes_nothing() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(95, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let binary = artifact(95);
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(246));
    let (environment_run, _publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(247), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let (receipt, mut launch) =
        commit_receipt(&cx, &root, &reservation, &environment_run, &mut session_run)
            .expect("receipt");
    // The supervisor loses its listener between publishing the context and
    // committing: the lease a stopped launch held is not custody any more.
    launch.stop();
    let error = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect_err("a stopped launch cannot publish a session");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
    assert_eq!(obligation_row(&reader, &environment), None);
    assert_eq!(plan.environment_generation(), Generation::new(2));
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    // The failed launch still held the session's custody.
    drop(launch);
    drop(session_run);
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(246),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("settle the failed successor");
}

#[test]
fn a_context_document_for_another_activation_is_refused() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let inputs = publish_inputs(96, None);
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let binary = artifact(96);
    let (mut session_run, _session) = admit_staged(&cx, &root, &reservation, run_id(248));
    let (environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(249), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let selection_digest = receipt_selection(&reservation);
    let mut good_launch = launch_for(&mut session_run, &plan.session()).expect("custody");
    let good = selection_receipt(
        &plan,
        publication.clone(),
        binary,
        None,
        &selection_digest,
        &session_run,
        &mut good_launch,
    );
    // The document a prepare would write for this activation, byte for byte.
    let base_document = context_document(
        &plan,
        &publication,
        good.selection(),
        &{
            let mut launch = launch_for(&mut session_run, &plan.session()).expect("custody");
            launch
                .qualify(&mut verifying_probe(), Duration::from_secs(1))
                .expect("qualified");
            launch
                .publish(&session_run)
                .expect("published")
                .lease()
                .clone()
        },
        &selection_digest,
    );
    let mut cases = Vec::new();
    for (field, value) in [
        ("schema", serde_json::json!("effigy.workspace-context.v1")),
        ("environment_generation", serde_json::json!(99)),
        ("context_generation", serde_json::json!(42)),
        ("version", serde_json::json!("0.0.0-foreign")),
        (
            "paths",
            serde_json::json!({"host_checkout":"/host/other","guest_checkout":"/workspace","workdir":"/workspace","user":"1000","home":"/home/worker"}),
        ),
        ("sources", serde_json::json!([])),
        ("binary_path", serde_json::json!("/run/effigy/bin/current")),
        ("backend", serde_json::json!("foreign-backend")),
        (
            "artifact_digest",
            serde_json::json!(format!("sha256:{}", "ff".repeat(32))),
        ),
        (
            "locator",
            serde_json::json!({"endpoint": "127.0.0.1:1", "pin": "00".repeat(32)}),
        ),
    ] {
        let mut parsed: serde_json::Value =
            serde_json::from_slice(&base_document).expect("document");
        parsed[field] = value;
        cases.push(serde_json::to_vec(&parsed).expect("document"));
    }
    let mut session_mutated: serde_json::Value =
        serde_json::from_slice(&base_document).expect("document");
    session_mutated["session"]["run_id"] = serde_json::json!(run_id(250).to_string());
    cases.push(serde_json::to_vec(&session_mutated).expect("document"));
    for bytes in cases {
        let mut selection = good.selection().clone();
        selection.objects.context.digest = Digest::of(&bytes);
        let mut launch = launch_for(&mut session_run, &plan.session()).expect("custody");
        let error = SelectionReceipt::new(
            &plan,
            publication.clone(),
            selection,
            &bytes,
            &session_run,
            &mut launch,
            &mut FakeSelectors::default(),
        )
        .expect_err("a context for another activation is never published");
        assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    }
    let reader = read_journal(&cx, &root);
    assert_eq!(mapping(&reader, &environment), None);
    drop(session_run);
    run::settle_interrupted(
        &cx,
        &root,
        &workspace_entry(),
        run_id(248),
        next(),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("settle the unused successor");
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn recovery_observes_under_the_environment_gate() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (_, _session_run, _, environment_run, receipt, obligation, launch) =
        commit_pending_selection(
            &cx,
            &root,
            &environment,
            run_id(251),
            run_id(252),
            &publish_inputs(97, None),
        );
    // The activation run still holds `Gate(Env)`: recovery must not take a
    // readback behind its back, so it refuses before it reads anything.
    #[derive(Debug, Default)]
    struct UnreadableReaders;
    impl SelectorReader for UnreadableReaders {
        fn target(&self) -> ContainerIdentity {
            published_target()
        }
        fn read_selector(&mut self, _path: &str) -> Observed<String> {
            panic!("recovery read the target outside the environment gate")
        }
        fn read_object(&mut self, _path: &str) -> Observed<Digest> {
            panic!("recovery read an object outside the environment gate")
        }
    }
    let error = selection_recovered(
        &cx,
        &root,
        &obligation,
        &launch,
        &mut verifying_target(),
        &mut verifying_probe(),
        &mut UnreadableReaders,
        Wait::FailFast,
    )
    .expect_err("the environment gate is held by the activation run");
    assert!(matches!(error, RunError::Gate(_)), "{error:?}");
    // Only after the activation run releases the gate does recovery read and
    // complete.
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    selection_recovered(
        &cx,
        &root,
        &obligation,
        &launch,
        &mut verifying_target(),
        &mut verifying_probe(),
        &mut FakeSelectors::exact(&receipt),
        Wait::FailFast,
    )
    .expect("recovery completes under the gate it acquired");
    assert!(una_finished(&read_journal(&cx, &root), &environment).is_none());
}

#[test]
fn a_fresh_project_freezes_the_artifact_its_first_publication_selected() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let binary = artifact(98);
    let inputs = PublishInputs {
        platform: None,
        guest: Some(fake_guest_context()),
        artifact: ArtifactEvidence::Unselected {
            image_reference: "registry.local/workspace:1".to_owned(),
            backend: "docker-desktop".to_owned(),
        },
        selection: Digest::of(b"fresh selection"),
        inputs_digest: Digest::of(b"fresh inputs"),
        target: TargetEvidence::Absent {
            project: "fixture".to_owned(),
            backend: "docker-desktop".to_owned(),
        },
    };
    let reservation = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision")
    {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(_) => panic!("fresh environment"),
    };
    let (mut session_run, session) = admit_staged(&cx, &root, &reservation, run_id(253));
    let (mut environment_run, publication) =
        admit_activation_publishing(&cx, &root, &reservation, run_id(254), &binary);
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).expect("claimed plan");
    let mut launch = launch_for(&mut session_run, &plan.session()).expect("custody");
    let receipt = selection_receipt(
        &plan,
        publication,
        binary.clone(),
        None,
        &inputs.selection,
        &session_run,
        &mut launch,
    );
    let committed = reservation
        .commit_selection(
            &cx,
            &root,
            &environment_run,
            &session_run,
            &receipt,
            &launch,
        )
        .expect("a fresh project publishes the artifact it selected");
    assert_eq!(committed.session(), session);
    let reader = read_journal(&cx, &root);
    let stored = WorkspaceActivationReservation::recover(&reader, &environment)
        .verified()
        .cloned()
        .flatten()
        .expect("the reservation")
        .record()
        .inputs
        .clone()
        .expect("frozen inputs");
    match stored.artifact {
        ArtifactEvidence::Selected { digest, .. } => {
            assert_eq!(digest, binary, "the selected artifact is frozen once");
        }
        ArtifactEvidence::Unselected { .. } => panic!("the first publication freezes the artifact"),
    }
    // The switch completes the selection, so the published generation is
    // attachable and the next decision compares against the frozen artifact.
    Attachments::selection_switched(
        &mut environment_run,
        committed
            .selection_obligation()
            .expect("the commit records its selection"),
        &launch,
        &mut FakeSelectors::exact(&receipt),
    )
    .expect("the switch is recorded");
    // The activation run finishes and releases `Gate(Env)`.
    end_run(
        &cx,
        environment_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
    // The next decision sees a selected artifact, so the same unselected intent
    // is a successor, never a re-publication of the frozen generation.
    let next = match WorkspaceActivationReservation::decide(
        &cx,
        &root,
        &environment,
        &inputs,
        Wait::FailFast,
    )
    .expect("decision after the first publication")
    {
        ActivationDecision::Activate(reservation) => reservation,
        ActivationDecision::Attach(_) => panic!("an unselected intent never attaches"),
    };
    assert_eq!(
        next.generation(),
        committed
            .plan()
            .environment_generation()
            .next()
            .expect("E+1")
    );
    next.cancel(&cx, &root, Wait::FailFast)
        .expect("release the unused successor");
    end_run(
        &cx,
        session_run,
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    );
}

#[test]
fn a_dead_session_is_never_attached_to() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let environment = pending_environment(&cx, &root);
    let inputs = publish_inputs(99, None);
    let activated =
        activate_with_selection(&cx, &root, &environment, run_id(255), run_id(254), &inputs);
    let session = activated.session.clone();
    // The session's owner is gone: its private slot is free, so the mapping is
    // an interrupted session, not a live one.
    drop(activated);
    let decision =
        WorkspaceActivationReservation::decide(&cx, &root, &environment, &inputs, Wait::FailFast)
            .expect("a dead session is replaceable, never attachable");
    let reservation = match decision {
        ActivationDecision::Activate(reservation) => *reservation,
        ActivationDecision::Attach(found) => {
            panic!("a dead session must not be attached: {found:?}")
        }
    };
    assert_eq!(
        reservation.generation(),
        session.environment_generation().next().expect("E+1"),
        "the successor is the generation after the dead session"
    );
    reservation
        .cancel(&cx, &root, Wait::FailFast)
        .expect("release the unused successor");
    assert_ne!(reservation.id().to_string(), String::new());
}

#[test]
fn an_unprobed_listener_and_a_released_owner_never_serve() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(78);
    // A listener that arrives already advertising a serving phase still cannot
    // mint a locator: only this launch's own verified handshake qualifies it.
    let (mut listener, stops) = FakeListener::bound();
    listener.phase = ListenerPhase::Holding;
    let key = ServingKey::new(vec![8u8; 32], Digest::of(b"unprobed pin")).expect("key");
    let mut launch = SessionLaunch::launch(
        &mut session_run,
        &session,
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    assert!(
        matches!(
            launch.publish(&session_run),
            Err(ServingRefusal::Unqualified)
        ),
        "a phase flag is not a qualification"
    );
    // An unanswered probe qualifies nothing either.
    assert!(matches!(
        launch.qualify(
            &mut FakeProbe {
                answer: Some(Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "workspace-session-listener".into(),
                })),
                calls: 0,
            },
            Duration::from_secs(1)
        ),
        Err(ServingRefusal::Unqualified)
    ));
    assert!(matches!(
        launch.publish(&session_run),
        Err(ServingRefusal::Unqualified)
    ));
    launch
        .qualify(&mut verifying_probe(), Duration::from_secs(1))
        .expect("qualified");
    let lease = launch
        .publish(&session_run)
        .expect("published")
        .lease()
        .clone();
    launch.serve(&session_run).expect("adopted");
    assert!(launch.serving(&session_run));
    // Releasing the owning run's custody leaves nothing to serve, even though
    // the listener and key are still here.
    launch.abandon_owner();
    assert!(!launch.key_held());
    assert!(!launch.owned());
    assert!(!launch.serving(&session_run));
    assert!(matches!(
        launch.serve(&session_run),
        Err(ServingRefusal::WrongSession)
    ));
    assert!(matches!(
        launch.publish(&session_run),
        Err(ServingRefusal::WrongSession)
    ));
    let probed = launch.liveness(&session_run, &mut verifying_probe(), Duration::from_secs(1));
    assert!(
        matches!(probed, Observed::Absent(_)),
        "a released owner has no served locator"
    );
    assert_eq!(launch.lease(), None);
    assert_eq!(
        stops.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "abandonment releases the listener immediately"
    );
    launch.release();
    assert_eq!(stops.load(std::sync::atomic::Ordering::SeqCst), 1);
    drop(session_run);
    let _ = lease;
}

#[test]
fn a_settled_session_stops_serving() {
    let fixture = ServingFixture::new();
    let (mut session_run, session) = fixture.admit(79);
    // A real socket, so settlement can be proven to close it.
    let listener =
        HostChannelListener::bind("127.0.0.1:0".parse().expect("address")).expect("bind");
    let endpoint = listener.endpoint().expect("bound endpoint");
    let key = ServingKey::new(vec![6u8; 32], Digest::of(b"settle pin")).expect("key");
    let mut launch = SessionLaunch::launch(
        &mut session_run,
        &session,
        "supervisor-1",
        key,
        Box::new(listener),
    )
    .expect("launch");
    launch
        .qualify(&mut verifying_probe(), Duration::from_secs(1))
        .expect("qualified");
    launch.publish(&session_run).expect("published");
    launch.serve(&session_run).expect("adopted");
    assert!(launch.serving(&session_run));
    // The session run starts settling: its fence is no longer open, so the
    // locator it published may not be called until it is settled.
    session_run
        .interrupt(
            &cleanup(&fixture.cx),
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        )
        .expect("interrupt the session");
    assert!(
        !launch.serving(&session_run),
        "a settling session serves nothing"
    );
    assert!(matches!(
        launch.serve(&session_run),
        Err(ServingRefusal::WrongSession)
    ));
    assert!(
        matches!(
            launch.liveness(&session_run, &mut verifying_probe(), Duration::from_secs(1)),
            Observed::Absent(_)
        ),
        "a settling session has no servable locator"
    );
    session_run
        .settle(&cleanup(&fixture.cx), &mut adapter())
        .expect("settle the session");
    // Settlement released the listener and the key: the socket no longer
    // accepts and the launch holds no key.
    assert!(
        std::net::TcpStream::connect(endpoint).is_err(),
        "settlement closes the published socket"
    );
    assert!(!launch.key_held(), "settlement wipes the session key");
    assert!(!launch.owned(), "settlement releases the owner custody");
    assert_eq!(launch.lease(), None, "a settled owner has no locator");
    // Settlement released the session's gate: a successor activation can take
    // the environment's next generation rather than meeting a held slot.
    let decision = WorkspaceActivationReservation::decide(
        &fixture.cx,
        &fixture.root,
        &fixture.environment,
        &publish_inputs(101, None),
        Wait::FailFast,
    )
    .expect("the settled session holds no gate");
    assert!(matches!(decision, ActivationDecision::Activate(_)));
    launch.release();
}

#[test]
fn recovery_refuses_a_target_that_is_not_the_published_container() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, _predecessor) = active_environment(&cx, &root);
    let (_, _session_run, _session, environment_run, receipt, obligation, launch) =
        commit_pending_selection(
            &cx,
            &root,
            &environment,
            run_id(252),
            run_id(253),
            &publish_inputs(100, None),
        );
    finish_activation(&cx, environment_run);
    // The target the publication names is gone (a replaced container), even
    // though the session, listener and readback would all verify.
    let mut replaced = FakeTarget {
        answer: Some(Observed::Absent(crate::observe::AbsenceProof::NoRow {
            table: "container".into(),
        })),
    };
    let error = selection_recovered(
        &cx,
        &root,
        &obligation,
        &launch,
        &mut replaced,
        &mut verifying_probe(),
        &mut FakeSelectors::exact(&receipt),
        Wait::FailFast,
    )
    .expect_err("a replaced target never completes a generation");
    assert!(matches!(error, RunError::Refused(_)), "{error:?}");
    let reader = read_journal(&cx, &root);
    assert_eq!(
        obligation_row(&reader, &environment).map(|row| row.0),
        Some("pending".to_owned()),
        "the obligation stays Pending for a live recovery"
    );
}

#[test]
fn custody_survives_the_activation_clients_eof() {
    let fixture = ServingFixture::new();
    std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let (commands, requests) = std::sync::mpsc::sync_channel(1);
        let fixture = &fixture;
        let owner = scope.spawn(move || {
            let (run, session) = fixture.admit(80);
            let mut supervisor =
                SessionSupervisor::adopt(run, "independent-supervisor").expect("handoff");
            let listener = HostChannelListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let key = ServingKey::new(vec![4; 32], Digest::of(b"handoff pin")).unwrap();
            let mut launch = SessionLaunch::launch(
                &mut supervisor,
                &session,
                "independent-supervisor",
                key,
                Box::new(listener),
            )
            .unwrap();
            launch
                .qualify(&mut verifying_probe(), Duration::from_secs(1))
                .unwrap();
            let lease = launch.publish(&supervisor).unwrap().lease().clone();
            launch.serve(&supervisor).unwrap();
            let ticket = match run::reserve(
                &fixture.cx,
                &fixture.root,
                run::AdmissionKey {
                    session: session.session(),
                    request: run::RequestId::new([82; 16]),
                },
                Digest::of(b"secondary task"),
                run_id(82),
            )
            .unwrap()
            {
                run::Reservation::Fresh(ticket) => ticket,
                _ => panic!("fresh admission"),
            };
            let workload = Run::begin_admitted(
                &fixture.cx,
                &fixture.root,
                &inv(task("secondary-task"), "secondary-task"),
                opts(run_id(82)),
                &cleanup(&fixture.cx),
                &mut adapter(),
                ticket,
            )
            .unwrap();
            tx.send((session, lease.clone())).unwrap();
            requests.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(launch.publish(&supervisor).unwrap().lease(), &lease);
            assert!(launch
                .liveness(&supervisor, &mut verifying_probe(), Duration::from_secs(1))
                .verified()
                .is_some());
            end_run(&fixture.cx, workload, WorkloadOutcome::NotStarted);
            supervisor
                .interrupt(&cleanup(&fixture.cx), WorkloadOutcome::NotStarted)
                .unwrap();
            supervisor
                .settle(&cleanup(&fixture.cx), &mut adapter())
                .unwrap();
            assert!(!launch.key_held());
            assert!(launch.lease().is_none());
            assert!(std::net::TcpStream::connect(lease.endpoint()).is_err());
        });
        // This client owns no Run or launch, and closes its own channel at EOF.
        let (session, lease) = {
            let caller = rx;
            caller.recv_timeout(Duration::from_secs(10)).unwrap()
        };
        assert!(std::net::TcpStream::connect(lease.endpoint()).is_ok());
        // A secondary attachment resolves the independently retained owner.
        let reader = read_journal(&fixture.cx, &fixture.root);
        assert_eq!(
            resolved(&reader, &fixture.environment),
            Some(session.clone())
        );
        let attachment = run::AttachmentId::new([83; 16]);
        let _secondary = run::attach_secondary(
            &fixture.cx,
            &fixture.root,
            session.session(),
            run_id(82),
            attachment,
        )
        .expect("secondary attaches after caller EOF");
        let _reconnected = run::resume_secondary(
            &fixture.cx,
            &fixture.root,
            session.session(),
            run_id(82),
            attachment,
        )
        .expect("same secondary reconnects");
        commands.send(()).unwrap();
        owner.join().unwrap();
    });
}

#[test]
fn dropping_an_unsettled_owner_closes_its_socket_and_wipes_its_key() {
    let fixture = ServingFixture::new();
    let (mut run, session) = fixture.admit(81);
    let listener = HostChannelListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let endpoint = listener.endpoint().unwrap();
    let key = ServingKey::new(vec![5; 32], Digest::of(b"drop pin")).unwrap();
    let mut launch =
        SessionLaunch::launch(&mut run, &session, "supervisor-1", key, Box::new(listener)).unwrap();
    launch
        .qualify(&mut verifying_probe(), Duration::from_secs(1))
        .unwrap();
    launch.publish(&run).unwrap();
    assert!(std::net::TcpStream::connect(endpoint).is_ok());
    drop(run);
    assert!(!launch.key_held());
    assert!(!launch.owned());
    assert!(launch.lease().is_none());
    assert!(std::net::TcpStream::connect(endpoint).is_err());
}

/// The guest read can overlap supervisor loss; the journal may not mistake
/// the still-open run row for custody after the read finishes.
struct OwnerLossReader<'a> {
    owner: Option<Run<'a>>,
    inner: FakeSelectors,
}
impl SelectorReader for OwnerLossReader<'_> {
    fn target(&self) -> ContainerIdentity {
        self.inner.target()
    }
    fn read_selector(&mut self, path: &str) -> Observed<String> {
        drop(self.owner.take());
        self.inner.read_selector(path)
    }
    fn read_object(&mut self, path: &str) -> Observed<Digest> {
        self.inner.read_object(path)
    }
}

#[test]
fn owner_loss_between_commit_and_switch_keeps_the_selection_pending() {
    for during_readback in [false, true] {
        let cx = context();
        let fixture = home();
        let root = StateRoot::effects(&cx, fixture.path()).unwrap();
        let (environment, _predecessor) = active_environment(&cx, &root);
        let (_, session_run, _, mut environment_run, receipt, obligation, launch) =
            commit_pending_selection(
                &cx,
                &root,
                &environment,
                run_id(84),
                run_id(85),
                &publish_inputs(101, None),
            );
        let mut reader = OwnerLossReader {
            owner: Some(session_run),
            inner: FakeSelectors::exact(&receipt),
        };
        if !during_readback {
            drop(reader.owner.take());
        }
        assert!(Attachments::selection_switched(
            &mut environment_run,
            &obligation,
            &launch,
            &mut reader
        )
        .is_err());
        assert!(!launch.key_held());
        assert_eq!(
            obligation_row(&read_journal(&cx, &root), &environment)
                .unwrap()
                .0,
            "pending"
        );
        end_run(&cx, environment_run, WorkloadOutcome::NotStarted);
    }
}

/// The transport can report independent failure or endpoint loss without the
/// supervisor calling SessionLaunch::stop (which clears the cached locator).
#[derive(Debug)]
struct FailingListener {
    inner: FakeListener,
    failure: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl ChannelListener for FailingListener {
    fn endpoint(&self) -> Option<std::net::SocketAddr> {
        match self.failure.load(std::sync::atomic::Ordering::SeqCst) {
            2 => None,
            3 => Some("127.0.0.1:46000".parse().unwrap()),
            _ => self.inner.endpoint(),
        }
    }
    fn phase(&self) -> ListenerPhase {
        if self.failure.load(std::sync::atomic::Ordering::SeqCst) == 1 {
            ListenerPhase::Stopped
        } else {
            self.inner.phase()
        }
    }
    fn qualify(&mut self) -> Result<(), ServingRefusal> {
        self.inner.qualify()
    }
    fn stop(&mut self) {
        self.inner.stop();
    }
}

#[test]
fn independent_listener_failure_refuses_cached_publication_commit_and_switch() {
    for failure_mode in 1..=3 {
        for committed in [false, true] {
            let cx = context();
            let fixture = home();
            let root = StateRoot::effects(&cx, fixture.path()).unwrap();
            let environment = pending_environment(&cx, &root);
            let inputs = publish_inputs(102, None);
            let reservation = match WorkspaceActivationReservation::decide(
                &cx,
                &root,
                &environment,
                &inputs,
                Wait::FailFast,
            )
            .unwrap()
            {
                ActivationDecision::Activate(r) => *r,
                _ => panic!("fresh activation"),
            };
            let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(86));
            let binary = inputs.artifact.selected().unwrap().clone();
            let (mut environment_run, publication) =
                admit_activation_publishing(&cx, &root, &reservation, run_id(87), &binary);
            let plan = plan_of(&reservation, &read_journal(&cx, &root)).unwrap();
            let failure = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let (inner, _) = FakeListener::bound();
            let key = ServingKey::new(vec![8; 32], Digest::of(b"independent failure")).unwrap();
            let mut launch = SessionLaunch::launch(
                &mut session_run,
                &plan.session(),
                "supervisor-1",
                key,
                Box::new(FailingListener {
                    inner,
                    failure: failure.clone(),
                }),
            )
            .unwrap();
            let receipt = selection_receipt(
                &plan,
                publication,
                binary,
                None,
                &receipt_selection(&reservation),
                &session_run,
                &mut launch,
            );
            let obligation = if committed {
                Some(
                    reservation
                        .commit_selection(
                            &cx,
                            &root,
                            &environment_run,
                            &session_run,
                            &receipt,
                            &launch,
                        )
                        .unwrap()
                        .selection_obligation()
                        .unwrap()
                        .clone(),
                )
            } else {
                None
            };
            failure.store(failure_mode, std::sync::atomic::Ordering::SeqCst);
            assert!(
                launch.publish(&session_run).is_err(),
                "no cached ChannelBound after transport failure"
            );
            assert!(launch.lease().is_none());
            assert!(launch.serve(&session_run).is_err());
            assert!(!launch.serving(&session_run));
            if let Some(obligation) = obligation {
                assert!(Attachments::selection_switched(
                    &mut environment_run,
                    &obligation,
                    &launch,
                    &mut FakeSelectors::exact(&receipt)
                )
                .is_err());
                assert_eq!(
                    obligation_row(&read_journal(&cx, &root), &environment)
                        .unwrap()
                        .0,
                    "pending"
                );
            } else {
                assert!(reservation
                    .commit_selection(
                        &cx,
                        &root,
                        &environment_run,
                        &session_run,
                        &receipt,
                        &launch
                    )
                    .is_err());
                assert!(mapping(&read_journal(&cx, &root), &environment).is_none());
                assert!(obligation_row(&read_journal(&cx, &root), &environment).is_none());
            }
            end_run(&cx, environment_run, WorkloadOutcome::NotStarted);
            end_run(&cx, session_run, WorkloadOutcome::NotStarted);
        }
    }
}

/// Deterministic recovery fault model. The existing boundary matrix above
/// covers decide/reserve, stage/admit, immutable publication, atomic commit and
/// switch crashes. This crosses recovery with owner loss, target substitution,
/// Unknown observations and retry, then exact settlement/abandonment.
#[test]
fn fault_model_recovery_custody_target_unknown_and_retry() {
    for fault in [
        "owner-during-read",
        "reader-B",
        "unknown-target",
        "unknown-listener",
        "listener-lost",
        "none",
    ] {
        let cx = context();
        let fixture = home();
        let root = StateRoot::effects(&cx, fixture.path()).unwrap();
        let environment = pending_environment(&cx, &root);
        let predecessor = activate_with_selection(
            &cx,
            &root,
            &environment,
            run_id(88),
            run_id(89),
            &publish_inputs(105, None),
        );
        let (_, session_run, session, environment_run, receipt, obligation, mut launch) =
            commit_pending_selection(
                &cx,
                &root,
                &environment,
                run_id(90),
                run_id(91),
                &publish_inputs(106, None),
            );
        finish_activation(&cx, environment_run);
        assert!(
            WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
                .is_err(),
            "public reserve must respect Pending"
        );
        let mut target = verifying_target();
        let mut probe = verifying_probe();
        let mut reader = FakeSelectors::exact(&receipt);
        if fault == "reader-B" {
            let mut other = published_target();
            other.vm_boot_id = "boot-B".into();
            reader.target = Some(other);
        }
        if fault == "unknown-target" {
            target.answer = Some(Observed::Unknown(UnknownReason::Unparsable(
                "unknown target".into(),
            )));
        }
        if fault == "unknown-listener" {
            probe.answer = Some(Observed::Unknown(UnknownReason::Unparsable(
                "unknown listener".into(),
            )));
        }
        if fault == "listener-lost" {
            launch.stop();
        }
        let mut owner = Some(session_run);
        let result = if fault == "owner-during-read" {
            selection_recovered(
                &cx,
                &root,
                &obligation,
                &launch,
                &mut target,
                &mut probe,
                &mut OwnerLossReader {
                    owner: owner.take(),
                    inner: reader,
                },
                Wait::FailFast,
            )
        } else {
            selection_recovered(
                &cx,
                &root,
                &obligation,
                &launch,
                &mut target,
                &mut probe,
                &mut reader,
                Wait::FailFast,
            )
        };
        let state = obligation_row(&read_journal(&cx, &root), &environment)
            .unwrap()
            .0;
        if fault == "none" {
            assert!(result.is_ok());
            assert_eq!(state, "switched");
        } else {
            assert!(result.is_err(), "{fault}");
            assert_eq!(state, "pending", "{fault}");
            if fault.starts_with("unknown") {
                assert!(matches!(result, Err(RunError::Unavailable(_))));
            }
            let retry = selection_recovered(
                &cx,
                &root,
                &obligation,
                &launch,
                &mut verifying_target(),
                &mut verifying_probe(),
                &mut FakeSelectors::exact(&receipt),
                Wait::FailFast,
            );
            if fault == "owner-during-read" || fault == "listener-lost" {
                assert!(retry.is_err());
            } else {
                assert!(retry.is_ok(), "transient recovery {fault}");
            }
        }
        if fault.starts_with("unknown") {
            let unknown = SelectionLiveness {
                session: Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "runs".into(),
                }),
                target: Observed::Unknown(UnknownReason::Unparsable("unknown".into())),
                listener: Observed::Verified((), evidence()),
            };
            assert!(selection_abandoned(
                &cx,
                &root,
                &obligation,
                &unknown,
                "unknown cannot revoke",
                Wait::FailFast
            )
            .is_err());
        }
        assert!(predecessor.session_run.requests().is_ok());
        assert_eq!(stop_requests(&cx, &root, predecessor.session.run()), 0);
        if let Some(run) = owner {
            end_run(&cx, run, WorkloadOutcome::NotStarted);
        } else {
            run::settle_interrupted(
                &cx,
                &root,
                &workspace_entry(),
                session.run(),
                next(),
                &cleanup(&cx),
                &mut adapter(),
            )
            .unwrap();
        }
        assert!(!launch.key_held());
        assert!(launch.lease().is_none());
        if fault == "owner-during-read" || fault == "listener-lost" {
            let dead = SelectionLiveness {
                session: Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "runs".into(),
                }),
                target: Observed::Verified((), evidence()),
                listener: Observed::Absent(crate::observe::AbsenceProof::NoRow {
                    table: "listener".into(),
                }),
            };
            selection_abandoned(
                &cx,
                &root,
                &obligation,
                &dead,
                "model settled owner",
                Wait::FailFast,
            )
            .unwrap();
            assert!(selection_abandoned(
                &cx,
                &root,
                &obligation,
                &dead,
                "stale retry",
                Wait::FailFast
            )
            .is_err());
            let reservation =
                WorkspaceActivationReservation::reserve(&cx, &root, &environment, Wait::FailFast)
                    .unwrap();
            reservation.cancel(&cx, &root, Wait::FailFast).unwrap();
        }
        end_run(&cx, predecessor.session_run, WorkloadOutcome::NotStarted);
    }
}

#[test]
fn deserialized_receipt_and_unobserved_prior_values_cannot_commit() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let environment = pending_environment(&cx, &root);
    let reservation = decided(&cx, &root, &environment);
    let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(92));
    let environment_run = admit_activation(&cx, &root, &reservation, run_id(93));
    let (receipt, mut launch) =
        commit_receipt(&cx, &root, &reservation, &environment_run, &mut session_run).unwrap();
    let mut json = serde_json::to_value(&receipt).unwrap();
    json["validated_document"] = serde_json::json!(true);
    json["selection"]["objects"]["context"]["digest"] =
        serde_json::to_value(Digest::of(b"forged document")).unwrap();
    let forged: SelectionReceipt = serde_json::from_value(json).unwrap();
    assert!(reservation
        .commit_selection(&cx, &root, &environment_run, &session_run, &forged, &launch)
        .is_err());
    assert!(mapping(&read_journal(&cx, &root), &environment).is_none());
    let plan = plan_of(&reservation, &read_journal(&cx, &root)).unwrap();
    let selection = receipt.selection().clone();
    let document = context_document(
        &plan,
        receipt.publication(),
        &selection,
        receipt.serving(),
        &receipt_selection(&reservation),
    );
    let mut prior = FakeSelectors::prior(&selection);
    prior.selectors.insert(
        selection.switches[0].path.clone(),
        Observed::Verified("unknown third".into(), evidence()),
    );
    assert!(SelectionReceipt::new(
        &plan,
        receipt.publication().clone(),
        selection,
        &document,
        &session_run,
        &mut launch,
        &mut prior
    )
    .is_err());
    end_run(&cx, environment_run, WorkloadOutcome::NotStarted);
    end_run(&cx, session_run, WorkloadOutcome::NotStarted);
    assert!(SessionRef::of_run(&read_journal(&cx, &root), run_id(92))
        .verified()
        .is_none());
}

fn target_candidate(run: RunId) -> TargetCandidate {
    TargetCandidate {
        identity: ContainerIdentity {
            container_id: "a1".repeat(32),
            vm_boot_id: "boot-0001".into(),
            backend: "docker-desktop".into(),
        },
        project: "fixture".into(),
        created_by: Some(run),
    }
}

#[test]
fn target_binding_is_exact_idempotent_and_preserves_decision() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).expect("root");
    let reservation = decided(&cx, &root, &environment());
    let mut run = Run::begin_claimed(
        &cx,
        &root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(211)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        reservation.claim().expect("claim"),
    )
    .expect("activation");
    let original = reservation.inputs().cloned();
    let candidate = target_candidate(run.id());
    for _ in 0..2 {
        assert_eq!(
            reservation
                .bind_target(
                    &mut run,
                    Observed::Verified(vec![candidate.clone()], evidence())
                )
                .expect("bound"),
            candidate.identity
        );
    }
    let mut stale = candidate.clone();
    stale.identity.vm_boot_id = "another-boot".into();
    assert!(reservation
        .bind_target(&mut run, Observed::Verified(vec![stale], evidence()))
        .is_err());
    assert_eq!(
        run.effects()
            .expect("effects")
            .iter()
            .filter(|effect| effect.spec.kind == "kernel:workspace-target")
            .count(),
        1
    );
    let Observed::Verified(Some(recovered), _) = WorkspaceActivationReservation::recover(
        &read_journal(&cx, &root),
        reservation.environment(),
    ) else {
        panic!("recover reservation")
    };
    assert_eq!(recovered.inputs(), original.as_ref());
}

#[test]
fn target_binding_refuses_ambiguous_unknown_and_foreign_creation_without_writing() {
    let home = home();
    let cx = context();
    let root = StateRoot::effects(&cx, home.path()).expect("root");
    let reservation = decided(&cx, &root, &environment());
    let mut run = Run::begin_claimed(
        &cx,
        &root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(212)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        reservation.claim().expect("claim"),
    )
    .expect("activation");
    let candidate = target_candidate(run.id());
    for candidates in [vec![], vec![candidate.clone(), candidate.clone()]] {
        assert!(reservation
            .bind_target(&mut run, Observed::Verified(candidates, evidence()))
            .is_err());
    }
    assert!(reservation
        .bind_target(
            &mut run,
            Observed::Unknown(UnknownReason::Inconsistent("inventory failed".into()))
        )
        .is_err());
    for change in 0..3 {
        let mut foreign = candidate.clone();
        match change {
            0 => foreign.created_by = Some(run_id(213)),
            1 => foreign.project = "foreign-project".into(),
            _ => foreign.identity.backend = "foreign-backend".into(),
        }
        assert!(reservation
            .bind_target(&mut run, Observed::Verified(vec![foreign], evidence()))
            .is_err());
    }
    assert!(run.effects().expect("effects").is_empty());
    reservation
        .bind_target(&mut run, Observed::Verified(vec![candidate], evidence()))
        .expect("valid retry");
}

#[cfg(feature = "fault-injection")]
#[test]
fn target_binding_lost_reply_retains_one_immutable_fact() {
    use crate::fault::{arm, arm_nth, fired, Fault, Point};
    for (point, count) in [(Point::IntendBefore, 0), (Point::IntendAfter, 1)] {
        let cx = context();
        let home = home();
        let root = StateRoot::effects(&cx, home.path()).expect("root");
        let reservation = decided(&cx, &root, &environment());
        let mut run = Run::begin_claimed(
            &cx,
            &root,
            &inv(reservation.environment().clone(), "environment:up"),
            opts(run_id(214)),
            || Ok(cleanup(&cx)),
            &mut adapter(),
            reservation.claim().expect("claim"),
        )
        .expect("activation");
        let candidate = target_candidate(run.id());
        arm_nth(point, 0, Fault::Enospc);
        assert!(reservation
            .bind_target(
                &mut run,
                Observed::Verified(vec![candidate.clone()], evidence())
            )
            .is_err());
        assert!(fired());
        arm(None);
        assert_eq!(run.effects().expect("effects").len(), count);
        let Observed::Verified(Some(recovered), _) = WorkspaceActivationReservation::recover(
            &read_journal(&cx, &root),
            reservation.environment(),
        ) else {
            panic!("recover reservation")
        };
        assert_eq!(
            recovered
                .bind_target(
                    &mut run,
                    Observed::Verified(vec![candidate.clone()], evidence())
                )
                .expect("retry"),
            candidate.identity
        );
        assert_eq!(run.effects().expect("effects").len(), 1);
    }
}

#[test]
fn target_binding_observed_identity_refuses_a_stale_boot_and_foreign_reservation() {
    let cx = context();
    let home = home();
    let root = StateRoot::effects(&cx, home.path()).expect("root");
    let mut candidate = target_candidate(run_id(215));
    let mut inputs = decision_inputs(90);
    inputs.target = TargetEvidence::Observed(candidate.identity.clone());
    let ActivationDecision::Activate(reservation) =
        WorkspaceActivationReservation::decide(&cx, &root, &environment(), &inputs, Wait::FailFast)
            .expect("decision")
    else {
        panic!("activate")
    };
    let mut run = Run::begin_claimed(
        &cx,
        &root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(215)),
        || Ok(cleanup(&cx)),
        &mut adapter(),
        reservation.claim().expect("claim"),
    )
    .expect("activation");
    let mut stale = candidate.clone();
    stale.identity.vm_boot_id = "replacement-boot".into();
    assert!(reservation
        .bind_target(&mut run, Observed::Verified(vec![stale], evidence()))
        .is_err());
    let mut foreign = (*reservation).clone();
    foreign.record.id = ActivationReservationId::parse(&"ff".repeat(16)).expect("id");
    assert!(foreign
        .bind_target(
            &mut run,
            Observed::Verified(vec![candidate.clone()], evidence())
        )
        .is_err());
    assert!(run.effects().expect("effects").is_empty());
    // An observed target predates this apply, so no creation claim is needed.
    candidate.created_by = None;
    reservation
        .bind_target(&mut run, Observed::Verified(vec![candidate], evidence()))
        .expect("observed target");
}

fn binary_activation<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    byte: u8,
) -> (WorkspaceActivationReservation, Run<'a>) {
    let reservation = decided(cx, root, &environment());
    let mut run = Run::begin_claimed(
        cx,
        root,
        &inv(reservation.environment().clone(), "environment:up"),
        opts(run_id(byte)),
        || Ok(cleanup(cx)),
        &mut adapter(),
        reservation.claim().expect("claim"),
    )
    .expect("activation");
    let candidate = target_candidate(run.id());
    reservation
        .bind_target(&mut run, Observed::Verified(vec![candidate], evidence()))
        .expect("target");
    (reservation, run)
}

fn binary_binding(run: RunId) -> BoundPlatform {
    let platform = ImagePlatform {
        os: "linux".into(),
        architecture: "arm64".into(),
        variant: "v8".into(),
        loader: LoaderFamily::Musl,
    };
    let digest = fake_artifact();
    BoundPlatform {
        target: target_candidate(run).identity,
        platform: platform.clone(),
        artifact_platform: platform,
        artifact: GuestObject {
            path: format!("/run/effigy/objects/bin/{digest}"),
            digest,
        },
        emulation_available: true,
        artifact_version: env!("CARGO_PKG_VERSION").into(),
    }
}

fn binary_receipt(stage: &GuestBinaryStage) -> GuestBinaryReceipt {
    GuestBinaryReceipt {
        target: stage.binding.target.clone(),
        platform: stage.binding.platform.clone(),
        object: stage.binding.artifact.clone(),
    }
}

#[test]
fn guest_stage_platform_is_image_selected_frozen_and_exact_target() {
    let cx = context();
    let home = home();
    let root = StateRoot::effects(&cx, home.path()).expect("root");
    let (reservation, mut run) = binary_activation(&cx, &root, 216);
    let binding = binary_binding(run.id());
    for mutation in 0..8 {
        let mut wrong = binding.clone();
        match mutation {
            0 => wrong.platform.os = "darwin".into(),
            1 => wrong.platform.architecture = "riscv64".into(),
            2 => wrong.platform.variant = "v7".into(),
            3 => wrong.artifact_platform.loader = LoaderFamily::Gnu,
            4 => wrong.emulation_available = false,
            5 => wrong.artifact.digest = Digest::of(b"wrong artifact"),
            6 => wrong.target.vm_boot_id = "stale-boot".into(),
            _ => wrong.artifact_version = "wrong-version".into(),
        }
        assert!(reservation
            .bind_platform(&mut run, Observed::Verified(wrong, evidence()))
            .is_err());
    }
    assert!(reservation
        .bind_platform(&mut run, Observed::Unknown(UnknownReason::Injected))
        .is_err());
    assert_eq!(
        reservation
            .bind_platform(&mut run, Observed::Verified(binding.clone(), evidence()))
            .expect("emulated arm64 image"),
        binding
    );
    assert_eq!(
        reservation
            .bind_platform(&mut run, Observed::Verified(binding.clone(), evidence()))
            .expect("replay"),
        binding
    );
    let mut replacement = binding.clone();
    replacement.platform.loader = LoaderFamily::Gnu;
    replacement.artifact_platform.loader = LoaderFamily::Gnu;
    assert!(reservation
        .bind_platform(&mut run, Observed::Verified(replacement, evidence()))
        .is_err());
    assert_eq!(
        reservation
            .guest_binary_stage(&run)
            .expect("frozen stage")
            .binding,
        binding
    );
    assert!(
        run.verified_workspace_binary().is_err(),
        "binding cannot substitute for a required stage"
    );
}

#[test]
fn guest_stage_intent_readback_unknown_and_recovery_never_select_or_retarget() {
    let cx = context();
    let home = home();
    let root = StateRoot::effects(&cx, home.path()).expect("root");
    let (reservation, mut run) = binary_activation(&cx, &root, 217);
    let binding = binary_binding(run.id());
    reservation
        .bind_platform(&mut run, Observed::Verified(binding, evidence()))
        .expect("platform");
    let stage = reservation.guest_binary_stage(&run).expect("stage");
    let mut foreign = stage.clone();
    foreign.binding.artifact.digest = Digest::of(b"wrong");
    assert!(Attachments::intend_guest_binary(&mut run, &foreign).is_err());
    let ticket = Attachments::intend_guest_binary(&mut run, &stage).expect("intent");
    let recovered_ticket =
        Attachments::intend_guest_binary(&mut run, &stage).expect("lost ticket recovered");
    assert_eq!(
        run.effects()
            .expect("effects")
            .iter()
            .filter(|effect| effect.spec.kind == run::GUEST_BINARY_KIND)
            .count(),
        1
    );
    Attachments::guest_binary_unknown(&mut run, &ticket, UnknownReason::Injected)
        .expect("partial copy uncertain");
    assert!(run.verified_workspace_binary().is_err());
    assert!(
        matches!(run::unfinished_guest_binary(&read_journal(&cx, &root), run.fence()), Observed::Verified(Some(ref unfinished), _) if unfinished == &stage)
    );
    let mut wrong = binary_receipt(&stage);
    wrong.object.digest = Digest::of(b"wrong bytes");
    assert!(Attachments::guest_binary_staged(
        &mut run,
        &recovered_ticket,
        Observed::Verified(wrong, evidence())
    )
    .is_err());
    assert!(run.verified_workspace_binary().is_err());
    Attachments::guest_binary_staged(
        &mut run,
        &recovered_ticket,
        Observed::Verified(binary_receipt(&stage), evidence()),
    )
    .expect("exact object hashed after interruption");
    Attachments::guest_binary_staged(
        &mut run,
        &recovered_ticket,
        Observed::Verified(binary_receipt(&stage), evidence()),
    )
    .expect("idempotent receipt");
    assert_eq!(run.verified_workspace_binary().expect("verified"), stage);
    assert!(matches!(
        run::unfinished_guest_binary(&read_journal(&cx, &root), run.fence()),
        Observed::Absent(_)
    ));
    assert!(
        matches!(
            SessionRef::resolve(&read_journal(&cx, &root), &environment()),
            Observed::Absent(_)
        ),
        "staging alone publishes nothing"
    );
}

#[cfg(feature = "fault-injection")]
#[test]
fn guest_stage_faults_preserve_intent_uncertainty_and_staged_orphans() {
    use crate::fault::{arm, arm_nth, fired, Fault, Point};
    for (step, point) in [
        (0, Point::IntendBefore),
        (0, Point::IntendAfter),
        (1, Point::IntendBefore),
        (1, Point::IntendAfter),
        (2, Point::AppliedBefore),
        (2, Point::AppliedAfter),
    ] {
        let cx = context();
        let home = home();
        let root = StateRoot::effects(&cx, home.path()).expect("root");
        let (reservation, mut run) = binary_activation(&cx, &root, 218);
        let binding = binary_binding(run.id());
        if step == 0 {
            arm_nth(point, 0, Fault::Enospc);
            assert!(reservation
                .bind_platform(&mut run, Observed::Verified(binding.clone(), evidence()))
                .is_err());
            assert!(fired());
            arm(None);
            reservation
                .bind_platform(&mut run, Observed::Verified(binding, evidence()))
                .expect("same platform replay");
        } else {
            reservation
                .bind_platform(&mut run, Observed::Verified(binding, evidence()))
                .expect("platform");
        }
        let stage = reservation.guest_binary_stage(&run).expect("stage");
        if step == 1 {
            arm_nth(point, 0, Fault::Enospc);
            assert!(Attachments::intend_guest_binary(&mut run, &stage).is_err());
            assert!(fired());
            arm(None);
        }
        let ticket = Attachments::intend_guest_binary(&mut run, &stage).expect("intent or replay");
        if step == 2 {
            arm_nth(point, 0, Fault::Enospc);
            assert!(Attachments::guest_binary_staged(
                &mut run,
                &ticket,
                Observed::Verified(binary_receipt(&stage), evidence())
            )
            .is_err());
            assert!(fired());
            arm(None);
        } else {
            Attachments::guest_binary_staged(
                &mut run,
                &ticket,
                Observed::Verified(binary_receipt(&stage), evidence()),
            )
            .expect("receipt");
        }
        let fence = run.fence().clone();
        drop(run); // interrupted before commit; bytes are additive orphans
        assert!(matches!(
            SessionRef::resolve(&read_journal(&cx, &root), &environment()),
            Observed::Absent(_)
        ));
        if step == 2 && point == Point::AppliedBefore {
            assert!(matches!(
                run::unfinished_guest_binary(&read_journal(&cx, &root), &fence),
                Observed::Verified(Some(_), _)
            ));
        }
    }
}

fn fake_guest_context() -> GuestContextInputs {
    GuestContextInputs {
        version: env!("CARGO_PKG_VERSION").into(),
        paths: GuestPaths {
            host_checkout: "/host/checkout".into(),
            guest_checkout: "/workspace".into(),
            workdir: "/workspace".into(),
            user: "1000".into(),
            home: "/home/worker".into(),
        },
        sources: vec![GuestSourcePin {
            kind: "composed-manifest".into(),
            name: "root".into(),
            digest: format!("sha256:{}", Digest::of(b"frozen manifest")),
            snapshot: Some("/run/effigy/sources/root.toml".into()),
        }],
    }
}

fn stage_fake_binary(
    reservation: &WorkspaceActivationReservation,
    run: &mut Run<'_>,
    digest: Digest,
) -> Result<(), RunError> {
    let target = published_target();
    let project = match &reservation.inputs().expect("inputs").target {
        TargetEvidence::Absent { project, .. } => project.clone(),
        _ => "fixture".into(),
    };
    let candidate = TargetCandidate {
        identity: target.clone(),
        project,
        created_by: Some(run.id()),
    };
    reservation.bind_target(run, Observed::Verified(vec![candidate], evidence()))?;
    let mut binding = binary_binding(run.id());
    binding.target = target;
    binding.artifact = GuestObject {
        path: format!("/run/effigy/objects/bin/{digest}"),
        digest,
    };
    reservation.bind_platform(run, Observed::Verified(binding, evidence()))?;
    let stage = reservation.guest_binary_stage(run)?;
    let ticket = Attachments::intend_guest_binary(run, &stage)?;
    Attachments::guest_binary_staged(
        run,
        &ticket,
        Observed::Verified(binary_receipt(&stage), evidence()),
    )?;
    Ok(())
}

#[test]
fn guest_stage_missing_or_unknown_cannot_replace_a_live_predecessor() {
    for unknown in [false, true] {
        let cx = context();
        let home = home();
        let root = StateRoot::effects(&cx, home.path()).expect("root");
        let environment = pending_environment(&cx, &root);
        let (_, _predecessor, first, activation) =
            activate(&cx, &root, &environment, run_id(219), run_id(220));
        finish_activation(&cx, activation);
        let reservation = decided(&cx, &root, &environment);
        let (mut session_run, _) = admit_staged(&cx, &root, &reservation, run_id(221));
        let mut run = Run::begin_claimed(
            &cx,
            &root,
            &inv(environment.clone(), "environment:up"),
            opts(run_id(222)),
            || Ok(cleanup(&cx)),
            &mut adapter(),
            reservation.claim().expect("claim"),
        )
        .expect("activation");
        let candidate = target_candidate(run.id());
        reservation
            .bind_target(&mut run, Observed::Verified(vec![candidate], evidence()))
            .expect("target");
        let binding = binary_binding(run.id());
        reservation
            .bind_platform(&mut run, Observed::Verified(binding, evidence()))
            .expect("platform");
        if unknown {
            let stage = reservation.guest_binary_stage(&run).expect("stage");
            let ticket = Attachments::intend_guest_binary(&mut run, &stage).expect("intent");
            Attachments::guest_binary_unknown(&mut run, &ticket, UnknownReason::Injected)
                .expect("uncertainty");
        }
        let ticket = Attachments::intend_guest_publication(&mut run).expect("publication intent");
        let receipt = publication_for(&cx, &root, &reservation, &run);
        Attachments::workspace_guest_published(&mut run, &ticket, receipt).expect("publication");
        assert!(commit_activation(&cx, &root, &reservation, &run, &mut session_run).is_err());
        assert_eq!(
            resolved(&read_journal(&cx, &root), &environment),
            Some(first)
        );
    }
}
