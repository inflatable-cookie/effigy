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
use std::path::Path;
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
    effigy_test_support::Fixture::in_parent("effigy-session-", Path::new("/tmp"))
        .expect("fixture home")
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
