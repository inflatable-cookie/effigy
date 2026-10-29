//! Fake-owner and fake-runtime proofs for the workspace session (§8.7/§8.11).
//!
//! These tests drive the kernel interface a supervisor and a dispatcher use:
//! one environment-owned `Detached` session run, request ids that replay after a
//! restart, terminals that attach without owning the run, bounded streams and a
//! down that refuses a foreign holder. No engine, container, gateway, socket
//! server or live supervisor is involved: the "fake owner" is a real kernel
//! `Run` in a fixture journal, and the "fake runtime" is a settlement adapter
//! that records nothing.

use super::*;
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, ObserveContext,
    OperationContext, StopCause,
};
use crate::id::{CheckoutId, CheckoutKind, ExecutionTarget, Route, Surface, TaskId};
use crate::observe::{AbsenceProof, Evidence, Observed};
use crate::outcome::{Invocation, WorkloadOutcome};
use crate::run::{
    Effect, EffectSpec, Reconciled, RequestAction, RunError, Settlement, SettlementAdapter,
};
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

/// One request's id and canonical digest, before any reservation is made.
fn request_identity(byte: u8, body: &[u8]) -> (RequestId, AdmissionRequest) {
    (
        RequestId::generate([byte; REQUEST_ID_BYTES]),
        AdmissionRequest::canonical(Method::Invoke, body),
    )
}

/// Reserves once and returns the winning ticket, refusing anything else.
fn reserve(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
    byte: u8,
    body: &[u8],
) -> AdmissionTicket {
    let (request, admission) = request_identity(byte, body);
    match SessionHandle::new(session.clone())
        .reserve(cx, root, &request, &admission)
        .expect("reservation")
    {
        ReserveDecision::Reserved(ticket) => ticket,
        other => panic!("expected a fresh reservation, got {other:?}"),
    }
}

fn admit_child<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    session: &SessionRef,
    session_run: &mut Run<'a>,
    byte: u8,
) -> (Run<'a>, AdmissionTicket) {
    admit_child_for(cx, root, session, session_run, byte, "dev")
}

fn admit_child_for<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    session: &SessionRef,
    session_run: &mut Run<'a>,
    byte: u8,
    name: &str,
) -> (Run<'a>, AdmissionTicket) {
    let ticket = reserve(cx, root, session, byte, name.as_bytes());
    let id = ticket.run().expect("an invoke reservation plans a run");
    let handle = SessionHandle::new(session.clone());
    let child = handle
        .admit_child(
            cx,
            root,
            session_run,
            &inv(task(name), name),
            opts(id),
            &cleanup(cx),
            &mut adapter(),
            &ticket,
        )
        .expect("child admission");
    (child, ticket)
}

/// One non-run request: its id and canonical digest, and the ticket a fresh
/// reservation wins.
struct PendingRequest {
    request: RequestId,
    admission: AdmissionRequest,
}
impl PendingRequest {
    fn new(method: Method, byte: u8, body: &[u8]) -> Self {
        Self {
            request: RequestId::generate([byte; REQUEST_ID_BYTES]),
            admission: AdmissionRequest::canonical(method, body),
        }
    }
    fn reserve(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        session: &SessionRef,
    ) -> AdmissionTicket {
        match SessionHandle::new(session.clone())
            .reserve(cx, root, &self.request, &self.admission)
            .expect("reservation")
        {
            ReserveDecision::Reserved(ticket) => ticket,
            other => panic!("expected a fresh reservation, got {other:?}"),
        }
    }
    fn replay(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        session: &SessionRef,
    ) -> ReserveDecision {
        SessionHandle::new(session.clone())
            .reserve(cx, root, &self.request, &self.admission)
            .expect("replay")
    }
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

fn authority(session: &SessionRef) -> (SessionAuthority, ClientGrant) {
    let binding = SessionBinding::new(
        "operator",
        "checkout-token",
        session.clone(),
        "container-1",
        "boot-1",
    )
    .expect("binding");
    let enrolment = Enrolment::new(
        session.environment_generation(),
        "container-1",
        "boot-1",
        vec![7; SECRET_BYTES],
    )
    .expect("enrolment");
    let mut authority = SessionAuthority::open(binding, enrolment.clone()).expect("authority");
    let capability = authority.enrol(&enrolment).expect("enrolment");
    let grant = authority.authenticate(&capability).expect("authentication");
    (authority, grant)
}

fn terminal(byte: u8) -> TerminalId {
    TerminalId::generate([byte; TERMINAL_ID_BYTES])
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
    let open = match crate::run::history(&reader, &workspace_entry(), 10) {
        Observed::Verified(runs, _) => runs
            .into_iter()
            .filter(|run| run.phase != Phase::Closed)
            .count(),
        other => panic!("history is not readable: {other:?}"),
    };
    assert_eq!(open, 1);
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
        Placement::Inline,
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
    let handle = SessionHandle::new(resolved);
    let (child, child_ticket) = admit_child(&cx, &root, &session, &mut session_run, 9);

    let reader = read_journal(&cx, &root);
    match crate::run::summary(&reader, child.id()) {
        Observed::Verified(summary, _) => assert_eq!(summary.parent, Some(session.run())),
        other => panic!("the child run is not readable: {other:?}"),
    }
    assert_eq!(
        child_ticket.run(),
        Some(child.id()),
        "the admission names the child it admitted"
    );
    assert_eq!(handle.identity(), &session);
    let open = match crate::run::history(&reader, &workspace_entry(), 10) {
        Observed::Verified(runs, _) => runs
            .into_iter()
            .filter(|run| run.phase != Phase::Closed)
            .count(),
        other => panic!("history is not readable: {other:?}"),
    };
    assert_eq!(open, 1, "attaching never admits a second session");
}

#[test]
fn a_retry_returns_the_recorded_run_and_never_admits_twice() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);

    let (request, admission) = request_identity(21, b"run the tests");
    let ticket = match SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("reservation")
    {
        ReserveDecision::Reserved(ticket) => ticket,
        other => panic!("expected a reservation, got {other:?}"),
    };
    let child = SessionHandle::new(session.clone())
        .admit_child(
            &cx,
            &root,
            &mut session_run,
            &inv(task("test"), "test"),
            opts(ticket.run().expect("an invoke reservation plans a run")),
            &cleanup(&cx),
            &mut adapter(),
            &ticket,
        )
        .expect("child admission");

    // The same digest is the recorded admission.
    let replay = SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("replay");
    match replay {
        ReserveDecision::Replayed(Admission::Admitted { run, placement, .. }) => {
            assert_eq!(run, child.id());
            assert_eq!(placement, Placement::Detached);
        }
        other => panic!("expected the recorded admission, got {other:?}"),
    }

    // The same id with another digest is a conflict, never a second run.
    let other = AdmissionRequest::canonical(Method::Invoke, b"run something else");
    let conflict = SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &other)
        .expect("conflict");
    match conflict {
        ReserveDecision::Conflict { .. } => {}
        other => panic!("expected a conflict, got {other:?}"),
    }
    let refusal = conflict.refusal().expect("a conflict is a refusal");
    assert_eq!(refusal.code(), RefusalCode::IdConflict);
    assert_eq!(refusal.code().as_str(), "id_conflict");

    let reader = read_journal(&cx, &root);
    let runs = match crate::run::history(&reader, &task("test"), 10) {
        Observed::Verified(runs, _) => runs,
        other => panic!("history is not readable: {other:?}"),
    };
    assert_eq!(runs.len(), 1, "one request id admits one run");
}

#[test]
fn a_completed_admission_replays_its_outcome() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut child, ticket) = admit_child(&cx, &root, &session, &mut session_run, 31);
    let child_id = child.id();

    child
        .finish(WorkloadOutcome::Succeeded)
        .expect("natural completion");
    let outcome = child.settle(&cleanup(&cx), &mut adapter()).expect("settle");
    SessionAdmissions::completed(
        &cx,
        &root,
        &ticket,
        &StoredOutcome::of(outcome.workload(), outcome.exit_code()),
    )
    .expect("recorded outcome");

    let (request, admission) = reserve_id(&ticket);
    let replay = SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("replay");
    match replay {
        ReserveDecision::Replayed(Admission::Completed { run, outcome, .. }) => {
            assert_eq!(run, child_id);
            assert_eq!(outcome.label(), "succeeded");
            assert_eq!(outcome.exit_code(), Some(0));
        }
        other => panic!("expected the recorded outcome, got {other:?}"),
    }
}

/// The request identity a ticket was reserved with, for a replay.
fn reserve_id(ticket: &AdmissionTicket) -> (RequestId, AdmissionRequest) {
    (
        ticket.key().request().clone(),
        AdmissionRequest::new(ticket.method(), ticket.digest().clone()),
    )
}

#[test]
fn a_reservation_survives_a_supervisor_restart() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (session_run, session) = admit_session(&cx, &root, &fence);

    // Reserve while the session is live: a fresh reader is a fresh supervisor,
    // and the same request is pending, never a fresh reservation.
    let (request, admission) = request_identity(41, b"run the dev profile");
    let planned = match SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("reservation")
    {
        ReserveDecision::Reserved(ticket) => ticket.run().expect("invoke plans a run"),
        other => panic!("expected a reservation, got {other:?}"),
    };
    let reader = read_journal(&cx, &root);
    match SessionAdmissions::lookup(&reader, &SessionKey::of(&session, request.clone())) {
        Observed::Verified(Some(record), _) => {
            assert_eq!(record.state(), AdmissionState::Reserved);
            assert_eq!(record.planned_run(), Some(planned));
        }
        other => panic!("the reservation is not readable: {other:?}"),
    }
    match SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("replay")
    {
        ReserveDecision::Replayed(Admission::Pending { planned_run }) => {
            assert_eq!(planned_run, Some(planned))
        }
        other => panic!("expected a pending replay, got {other:?}"),
    }

    // Now the supervisor is gone as well. The reservation survives, the run was
    // never admitted, and the retry is unknown rather than a new run.
    drop(session_run);
    match SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("replay")
    {
        ReserveDecision::Unknown { .. } => {}
        other => panic!("expected unknown, got {other:?}"),
    }
    let reader = read_journal(&cx, &root);
    assert!(
        matches!(
            crate::run::summary(&reader, planned),
            Observed::Absent(AbsenceProof::NoRow { .. })
        ),
        "an unresolved reservation never becomes a run"
    );
}

#[test]
fn an_interrupted_session_answers_unknown_not_a_new_run() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (session_run, session) = admit_session(&cx, &root, &fence);
    let (request, admission) = request_identity(51, b"run the dev profile");
    let _ = SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("reservation");
    // Crash the supervisor: the run row stays non-closed and its owner gate is
    // released, which is exactly `Interrupted`.
    drop(session_run);
    match SessionHandle::new(session.clone())
        .reserve(&cx, &root, &request, &admission)
        .expect("replay")
    {
        ReserveDecision::Unknown { reason } => {
            assert!(
                reason.contains("session"),
                "the reason names the session: {reason}"
            );
        }
        other => panic!("expected unknown, got {other:?}"),
    }
}

#[test]
fn terminals_attach_to_one_identity_and_only_the_first_gets_the_capability() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);

    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("tui attach");
    let text = authority
        .attach(&cx, &root, &grant, TerminalKind::Text, terminal(2), None)
        .expect("text attach");
    let headless = authority
        .attach(
            &cx,
            &root,
            &grant,
            TerminalKind::Headless,
            terminal(3),
            None,
        )
        .expect("headless attach");

    assert!(
        tui.session_capability().is_some(),
        "the first terminal gets the capability"
    );
    assert!(
        text.session_capability().is_none() && headless.session_capability().is_none(),
        "a later terminal never receives the session capability"
    );
    let labels: Vec<String> = [&tui, &text, &headless]
        .iter()
        .map(|terminal| terminal.identity().label())
        .collect();
    assert!(labels.iter().all(|label| label == &session.label()));
    assert_eq!(authority.attached().len(), 3);

    let reader = read_journal(&cx, &root);
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => {
            assert_eq!(rows.len(), 3);
            assert!(rows
                .iter()
                .all(|(_, _, state)| *state == TerminalState::Attached));
        }
        other => panic!("terminal records are not readable: {other:?}"),
    }
}

#[test]
fn a_secondary_disconnect_stops_nothing() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);

    let _first = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("first attach");
    let secondary = authority
        .attach(
            &cx,
            &root,
            &grant,
            TerminalKind::Headless,
            terminal(2),
            None,
        )
        .expect("secondary attach");
    let secondary_id = secondary.id().clone();
    drop(secondary);

    let reaped = authority.reap(&cx, &root).expect("reap");
    assert_eq!(reaped.len(), 1);
    assert_eq!(reaped[0].terminal(), &secondary_id);
    assert!(!reaped[0].released());
    assert!(reaped[0].owned().is_empty());
    assert!(is_live(&session_run), "the session keeps running");
    assert_eq!(
        authority.attached().len(),
        1,
        "the first terminal is still attached"
    );

    let reader = read_journal(&cx, &root);
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => {
            let lost = rows
                .iter()
                .find(|(id, _, _)| *id == secondary_id)
                .expect("the lost terminal is recorded");
            assert_eq!(lost.2, TerminalState::Lost);
        }
        other => panic!("terminal records are not readable: {other:?}"),
    }
    // The session's own journal records no stop request from a terminal.
    let requests = session_run.requests().expect("requests");
    assert!(requests.is_empty(), "a disconnect writes no stop request");

    // And it settles once: one settle, one closed record.
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
    match crate::run::summary(&reader, session.run()) {
        Observed::Verified(summary, _) => assert_eq!(summary.phase, Phase::Closed),
        other => panic!("the session is not readable: {other:?}"),
    }
}

#[test]
fn an_explicit_detach_leaves_the_session_running() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let mut tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");

    authority
        .detach(&cx, &root, &mut tui, Some(StreamCursor::at(4)))
        .expect("detach");
    drop(tui);
    assert!(authority.reap(&cx, &root).expect("reap").is_empty());
    assert!(is_live(&session_run), "a detach never stops the session");

    let _ = admit_child(&cx, &root, &session, &mut session_run, 61);
    let reader = read_journal(&cx, &root);
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].2, TerminalState::Detached);
        }
        other => panic!("terminal records are not readable: {other:?}"),
    }
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
    let requests = foreign.requests().expect("requests");
    assert!(requests.is_empty());
    assert!(is_live(&foreign), "the foreign run is untouched");
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
    let (mut child, _ticket) = admit_child(&cx, &root, &session, &mut session_run, 81);

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

    let stop = RequestId::generate([91; REQUEST_ID_BYTES]);
    children
        .stop(&cx, &root, &reader, &stop, child.id())
        .expect("child stop");
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

#[test]
fn cancel_is_recorded_once_and_only_for_owned_runs() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (child, _ticket) = admit_child(&cx, &root, &session, &mut session_run, 101);
    let (_authority, grant) = authority(&session);
    let handle = SessionHandle::new(session.clone());

    let request = RequestId::generate([111; REQUEST_ID_BYTES]);
    handle
        .cancel(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &grant,
            &request,
            child.id(),
        )
        .expect("authorised cancel");
    // The same request id is one journalled request: a retried cancel reads it
    // and the child still settles once.
    handle
        .cancel(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &grant,
            &request,
            child.id(),
        )
        .expect("the repeated cancel reads its recorded request");
    assert_eq!(stop_requests(&cx, &root, child.id()), 1);
    assert_eq!(
        child
            .requests()
            .expect("requests")
            .iter()
            .filter(|request| matches!(request.action, RequestAction::Stop))
            .count(),
        1
    );

    // A run the client does not own is refused before anything is written.
    let foreign = Run::begin(
        &cx,
        &root,
        &inv(task("someone:else"), "someone:else"),
        opts(run_id(112)),
        &cleanup(&cx),
        &mut adapter(),
    )
    .expect("foreign run");
    let foreign_request = RequestId::generate([113; REQUEST_ID_BYTES]);
    let error = handle
        .cancel(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &grant,
            &foreign_request,
            foreign.id(),
        )
        .expect_err("a foreign run is out of scope");
    assert!(matches!(error, RunError::Refused(_)));
    assert!(foreign.requests().expect("requests").is_empty());
}

#[test]
fn streams_resume_and_report_a_gap() {
    let mut stream = StreamBuffer::new(StreamId::parse("run:1").expect("stream"));
    for index in 0..3u8 {
        stream
            .push(StreamKind::Stdout, vec![b'a' + index])
            .expect("queued");
    }
    match stream.read(StreamCursor::start()) {
        ReadOutcome::Frames(frames) => {
            assert_eq!(frames.len(), 3);
            assert_eq!(frames[0].seq, 1);
            assert_eq!(frames[2].payload, b"c".to_vec());
        }
        other => panic!("expected frames, got {other:?}"),
    }
    match stream.read(StreamCursor::at(3)) {
        ReadOutcome::Frames(frames) => assert_eq!(frames.len(), 1),
        other => panic!("expected the resumed frame, got {other:?}"),
    }
    for _ in 0..300 {
        match stream.push(StreamKind::Stdout, b"x".to_vec()) {
            Ok(PushOutcome::Queued(seq)) => stream.ack(seq).expect("ack"),
            other => panic!("expected a queued frame, got {other:?}"),
        }
    }
    match stream.read(StreamCursor::start()) {
        ReadOutcome::Gap { from, to, resumed } => {
            assert_eq!(from, 1);
            assert!(to > from);
            assert_eq!(resumed, to);
            assert!(resumed > 1, "a gap is never presented as complete output");
        }
        other => panic!("expected a gap, got {other:?}"),
    }
    assert!(matches!(
        stream.read(StreamCursor::at(stream.cursor().sequence() + 1)),
        ReadOutcome::Refused(StreamError::InvalidCursor)
    ));
}

#[test]
fn terminal_input_is_never_replayed_and_resize_is_latest_wins() {
    let mut stream = StreamBuffer::new(StreamId::parse("run:1").expect("stream"));
    stream
        .push(StreamKind::TerminalOutput, b"out".to_vec())
        .expect("queued");
    assert_eq!(
        stream.push(StreamKind::TerminalInput, b"typed".to_vec()),
        Ok(PushOutcome::NotBuffered)
    );
    stream.input(b"typed".to_vec()).expect("delivered once");
    assert_eq!(stream.input_frames(), 1);

    stream.resize(24, 80).expect("first resize");
    stream.resize(40, 120).expect("second resize");
    assert_eq!(stream.pending_resize(), Some((40, 120)));
    assert!(matches!(
        stream.resize(0, 80),
        Err(StreamError::InvalidResize)
    ));

    // A reconnect reads output; input has no sequence to resume from.
    match stream.read(StreamCursor::start()) {
        ReadOutcome::Frames(frames) => {
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].kind, StreamKind::TerminalOutput);
        }
        other => panic!("expected output only, got {other:?}"),
    }
}

#[test]
fn backpressure_does_not_block_cancel() {
    let mut stream = StreamBuffer::new(StreamId::parse("run:1").expect("stream"));
    for _ in 0..MAX_UNACKNOWLEDGED_FRAMES {
        assert!(matches!(
            stream.push(StreamKind::Stdout, b"x".to_vec()),
            Ok(PushOutcome::Queued(_))
        ));
    }
    match stream.push(StreamKind::Stdout, b"x".to_vec()) {
        Ok(PushOutcome::Backpressured { unacknowledged, .. }) => {
            assert_eq!(unacknowledged, MAX_UNACKNOWLEDGED_FRAMES)
        }
        other => panic!("expected backpressure, got {other:?}"),
    }
    // Cancellation is control: it is serviceable while the data window is full.
    stream.cancel();
    assert!(stream.cancelled());
    // And the queued frames are still readable, so a cancelling client learns
    // what it already had.
    match stream.read(StreamCursor::start()) {
        ReadOutcome::Frames(frames) => assert_eq!(frames.len(), MAX_UNACKNOWLEDGED_FRAMES),
        other => panic!("expected the queued frames, got {other:?}"),
    }

    // An acknowledgement reopens the window.
    let mut stream = StreamBuffer::new(StreamId::parse("run:2").expect("stream"));
    for _ in 0..MAX_UNACKNOWLEDGED_FRAMES {
        stream
            .push(StreamKind::Stdout, b"x".to_vec())
            .expect("queued");
    }
    stream.ack(MAX_UNACKNOWLEDGED_FRAMES as u64).expect("ack");
    assert!(matches!(
        stream.push(StreamKind::Stdout, b"x".to_vec()),
        Ok(PushOutcome::Queued(_))
    ));
}

#[test]
fn the_byte_window_reopens_on_acknowledgement() {
    // Regression (review 5694163884): the 512 KiB window counted retained
    // bytes, so a stream of ~2 KiB frames filled it after 256 frames and then
    // backpressured for good — acknowledgements never released anything. A
    // frame is 2 KiB here, so 256 retained frames reach exactly 512 KiB.
    let mut stream = StreamBuffer::new(StreamId::parse("run:1").expect("stream"));
    let frame = vec![b'x'; 2048];
    let mut pushed = 0u64;
    for _ in 0..300 {
        match stream.push(StreamKind::Stdout, frame.clone()) {
            Ok(PushOutcome::Queued(seq)) => {
                stream.ack(seq).expect("ack");
                pushed += 1;
            }
            other => panic!("expected a queued frame, got {other:?}"),
        }
    }
    assert_eq!(pushed, 300, "a reader that keeps up is never stalled");
    assert_eq!(stream.unacknowledged(), 0);
    assert_eq!(
        stream.queued_bytes(),
        0,
        "an acknowledgement releases its bytes"
    );

    // Retention stays bounded independently: the resume window dropped the
    // oldest frames, so a cursor before it is a gap rather than a claim of
    // complete output.
    assert!(stream.retained_from() > 1);
    match stream.read(StreamCursor::start()) {
        ReadOutcome::Gap { from, to, resumed } => {
            assert_eq!(from, 1);
            assert_eq!(resumed, to);
        }
        other => panic!("expected a gap, got {other:?}"),
    }

    // The frame window is unchanged: 16 unacknowledged data frames still
    // backpressure, and one acknowledgement still reopens it.
    let mut stream = StreamBuffer::new(StreamId::parse("run:2").expect("stream"));
    for _ in 0..MAX_UNACKNOWLEDGED_FRAMES {
        stream
            .push(StreamKind::Stdout, frame.clone())
            .expect("queued");
    }
    assert_eq!(
        stream.queued_bytes(),
        MAX_UNACKNOWLEDGED_FRAMES * frame.len()
    );
    assert!(matches!(
        stream.push(StreamKind::Stdout, frame.clone()),
        Ok(PushOutcome::Backpressured { .. })
    ));
    stream.ack(MAX_UNACKNOWLEDGED_FRAMES as u64).expect("ack");
    assert_eq!(stream.queued_bytes(), 0);
    assert!(matches!(
        stream.push(StreamKind::Stdout, frame),
        Ok(PushOutcome::Queued(_))
    ));
}

#[test]
fn end_carries_the_final_sequence_and_outcome() {
    let mut stream = StreamBuffer::new(StreamId::parse("run:1").expect("stream"));
    stream
        .push(StreamKind::Stdout, b"x".to_vec())
        .expect("queued");
    let end = stream.end(Some("succeeded".into())).expect("end");
    assert_eq!(end, 2);
    match stream.read(StreamCursor::at(2)) {
        ReadOutcome::Frames(frames) => {
            assert_eq!(frames.len(), 1);
            assert_eq!(frames[0].kind, StreamKind::End);
        }
        other => panic!("expected the end frame, got {other:?}"),
    }
    assert_eq!(
        stream.read(StreamCursor::at(3)),
        ReadOutcome::End {
            seq: 2,
            outcome: Some("succeeded".into())
        }
    );
    assert!(matches!(
        stream.push(StreamKind::Stdout, b"late".to_vec()),
        Err(StreamError::Closed)
    ));
}

#[test]
fn enrolment_and_capability_are_generation_bound() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let binding = SessionBinding::new(
        "operator",
        "checkout-token",
        session.clone(),
        "container-1",
        "boot-1",
    )
    .expect("binding");
    let enrolment = Enrolment::new(
        session.environment_generation(),
        "container-1",
        "boot-1",
        vec![7; SECRET_BYTES],
    )
    .expect("enrolment");
    let mut authority = SessionAuthority::open(binding, enrolment.clone()).expect("authority");

    // Another container's credential enrols nothing.
    let elsewhere = Enrolment::new(
        session.environment_generation(),
        "container-2",
        "boot-1",
        vec![7; SECRET_BYTES],
    )
    .expect("enrolment");
    assert_eq!(
        authority.enrol(&elsewhere).expect_err("refused").code(),
        RefusalCode::IdentityMismatch
    );
    // A wrong secret is unauthenticated.
    let wrong = Enrolment::new(
        session.environment_generation(),
        "container-1",
        "boot-1",
        vec![8; SECRET_BYTES],
    )
    .expect("enrolment");
    assert_eq!(
        authority.enrol(&wrong).expect_err("refused").code(),
        RefusalCode::Unauthenticated
    );

    let capability = authority.enrol(&enrolment).expect("enrolled");
    let grant = authority.authenticate(&capability).expect("authenticated");
    assert!(grant.allows(Method::Invoke));
    let (_authority, _) = (authority, grant);

    // A new environment generation revokes the capability.
    let mut authority = SessionAuthority::open(
        SessionBinding::new(
            "operator",
            "checkout-token",
            session.clone(),
            "container-1",
            "boot-1",
        )
        .expect("binding"),
        enrolment.clone(),
    )
    .expect("authority");
    let capability = authority.enrol(&enrolment).expect("enrolled");
    let next = Generation::new(session.environment_generation().get() + 1);
    assert!(authority.revoke_on_generation(next));
    assert_eq!(
        authority
            .authenticate(&capability)
            .expect_err("revoked")
            .code(),
        RefusalCode::Unauthenticated
    );
    assert!(
        !authority.revoke_on_generation(session.environment_generation()),
        "the current generation revokes nothing"
    );
}

#[test]
fn a_terminal_record_survives_a_restart() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (environment, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let _tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");

    // A fresh supervisor reads the same session and the same attached terminal.
    let reader = read_journal(&cx, &root);
    assert_eq!(
        SessionRef::resolve(&reader, &environment)
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone())
    );
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].1, TerminalKind::Tui);
        }
        other => panic!("terminal records are not readable: {other:?}"),
    }
    // Stopping the session tells every attached terminal, once.
    let stopped = authority.stopping(&cx, &root).expect("stopping");
    assert_eq!(stopped, vec![terminal(1)]);
    assert!(authority.attached().is_empty());
    let reader = read_journal(&cx, &root);
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => assert_eq!(rows[0].2, TerminalState::Stopped),
        other => panic!("terminal records are not readable: {other:?}"),
    }
}

#[test]
fn a_non_run_reservation_never_plans_a_run() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (session_run, session) = admit_session(&cx, &root, &fence);

    let pending = PendingRequest::new(Method::Cancel, 171, b"stop the dev child");
    let ticket = pending.reserve(&cx, &root, &session);
    assert_eq!(
        ticket.run(),
        None,
        "a cancel plans no run, so a replay never names a phantom one"
    );

    match pending.replay(&cx, &root, &session) {
        ReserveDecision::Replayed(Admission::Pending { planned_run }) => {
            assert_eq!(planned_run, None)
        }
        other => panic!("expected a pending non-run request, got {other:?}"),
    }
    let reader = read_journal(&cx, &root);
    match SessionAdmissions::lookup(&reader, &SessionKey::of(&session, pending.request.clone())) {
        Observed::Verified(Some(record), _) => {
            assert_eq!(record.state(), AdmissionState::Reserved);
            assert_eq!(record.planned_run(), None);
            assert_eq!(record.admitted_run(), None);
        }
        other => panic!("the reservation is not readable: {other:?}"),
    }

    // With no live session the reservation cannot be resolved: unknown, never
    // a fresh request.
    drop(session_run);
    match pending.replay(&cx, &root, &session) {
        ReserveDecision::Unknown { .. } => {}
        other => panic!("expected unknown, got {other:?}"),
    }
}

#[test]
fn attach_replays_its_recorded_terminal() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);

    let pending = PendingRequest::new(Method::Attach, 181, b"attach a tui");
    let ticket = pending.reserve(&cx, &root, &session);
    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");
    let receipt = StoredOutcome::new(format!("attached:{}", tui.id()), None).expect("receipt");
    SessionAdmissions::applied(&cx, &root, &ticket, &receipt).expect("receipt recorded");

    match pending.replay(&cx, &root, &session) {
        ReserveDecision::Replayed(Admission::Applied { outcome }) => {
            assert_eq!(
                outcome.label(),
                format!("attached:{}", terminal(1).as_str())
            );
        }
        other => panic!("expected the recorded attachment, got {other:?}"),
    }
    // The receipt is the whole answer: a replay attaches nothing a second time.
    let reader = read_journal(&cx, &root);
    match terminals(&reader, &session) {
        Observed::Verified(rows, _) => assert_eq!(rows.len(), 1),
        other => panic!("terminal records are not readable: {other:?}"),
    }
}

#[test]
fn cancel_replays_and_a_repeated_id_is_one_request() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (child, _ticket) = admit_child(&cx, &root, &session, &mut session_run, 191);
    let (_authority, grant) = authority(&session);
    let handle = SessionHandle::new(session.clone());

    let pending = PendingRequest::new(Method::Cancel, 192, b"stop the dev child");
    let ticket = pending.reserve(&cx, &root, &session);
    assert_eq!(ticket.run(), None);

    handle
        .cancel(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &grant,
            &pending.request,
            child.id(),
        )
        .expect("cancel");
    // The same id again is the same durable request, not a second one.
    handle
        .cancel(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &grant,
            &pending.request,
            child.id(),
        )
        .expect("the repeated cancel reads its recorded request");
    assert_eq!(stop_requests(&cx, &root, child.id()), 1);

    let receipt = StoredOutcome::new(format!("stopped:{}", child.id()), Some(0)).expect("receipt");
    SessionAdmissions::applied(&cx, &root, &ticket, &receipt).expect("receipt recorded");
    match pending.replay(&cx, &root, &session) {
        ReserveDecision::Replayed(Admission::Applied { outcome }) => {
            assert_eq!(outcome.label(), format!("stopped:{}", child.id()));
        }
        other => panic!("expected the recorded cancellation, got {other:?}"),
    }
    assert_eq!(stop_requests(&cx, &root, child.id()), 1, "one settle");
}

#[test]
fn detach_replays_its_recorded_receipt() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (_session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let mut tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");

    let pending = PendingRequest::new(Method::Detach, 201, b"detach the tui");
    let ticket = pending.reserve(&cx, &root, &session);
    authority
        .detach(&cx, &root, &mut tui, Some(StreamCursor::at(3)))
        .expect("detach");
    let receipt = StoredOutcome::new(format!("detached:{}", tui.id()), None).expect("receipt");
    SessionAdmissions::applied(&cx, &root, &ticket, &receipt).expect("receipt recorded");

    match pending.replay(&cx, &root, &session) {
        ReserveDecision::Replayed(Admission::Applied { outcome }) => {
            assert_eq!(
                outcome.label(),
                format!("detached:{}", terminal(1).as_str())
            );
        }
        other => panic!("expected the recorded detach, got {other:?}"),
    }
    // A detach is a non-run operation: its record names no run.
    let reader = read_journal(&cx, &root);
    match SessionAdmissions::lookup(&reader, &SessionKey::of(&session, pending.request.clone())) {
        Observed::Verified(Some(record), _) => {
            assert_eq!(record.state(), AdmissionState::Completed);
            assert_eq!(record.admitted_run(), None);
        }
        other => panic!("the receipt is not readable: {other:?}"),
    }
}

#[test]
fn eof_stops_only_what_that_terminal_owned() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("tui attach");
    let text = authority
        .attach(&cx, &root, &grant, TerminalKind::Text, terminal(2), None)
        .expect("text attach");
    let (tui_id, text_id) = (tui.id().clone(), text.id().clone());
    let (child_a, _) = admit_child_for(&cx, &root, &session, &mut session_run, 211, "dev");
    let (child_b, _) = admit_child_for(&cx, &root, &session, &mut session_run, 212, "test");
    record_owned(&cx, &root, &session, &tui_id, child_a.id()).expect("tui owns child a");
    record_owned(&cx, &root, &session, &text_id, child_b.id()).expect("text owns child b");

    // EOF without a detach ends the runs this terminal admitted, and nothing
    // else: the other terminal's run and the session stay.
    let end = authority
        .owner_lost(&cx, &root, &tui_id, OwnerLoss::EndOfInput)
        .expect("eof");
    assert!(!end.released());
    assert_eq!(end.owned().to_vec(), vec![child_a.id()]);
    assert_eq!(end.cause(), StopCause::OwnerLost);
    assert_eq!(
        authority.attached().len(),
        1,
        "only the text terminal remains"
    );

    let handle = SessionHandle::new(session.clone());
    let stop = RequestId::generate([213; REQUEST_ID_BYTES]);
    handle
        .stop_owned(&cx, &root, &read_journal(&cx, &root), &stop, child_a.id())
        .expect("stop the owned child");
    // A retried owner loss asks for the same stop, not a second one.
    handle
        .stop_owned(&cx, &root, &read_journal(&cx, &root), &stop, child_a.id())
        .expect("the repeated stop reads its recorded request");
    assert_eq!(stop_requests(&cx, &root, child_a.id()), 1);
    assert!(child_b.requests().expect("requests").is_empty());
    assert!(is_live(&child_b));
    assert!(
        is_live(&session_run),
        "a terminal loss never ends the session"
    );
    assert!(
        handle
            .stop_owned(
                &cx,
                &root,
                &read_journal(&cx, &root),
                &RequestId::generate([214; REQUEST_ID_BYTES]),
                session.run(),
            )
            .is_err(),
        "a terminal loss cannot stop the session run"
    );
}

#[test]
fn eof_after_acknowledged_detach_stops_nothing() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let mut tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");
    let tui_id = tui.id().clone();
    let (child, _) = admit_child_for(&cx, &root, &session, &mut session_run, 221, "dev");
    record_owned(&cx, &root, &session, &tui_id, child.id()).expect("owned");

    authority
        .detach(&cx, &root, &mut tui, None)
        .expect("detach acknowledged");
    let end = authority
        .owner_lost(&cx, &root, &tui_id, OwnerLoss::EndOfInput)
        .expect("eof after detach");
    assert!(end.released(), "the terminal had detached");
    assert!(end.owned().is_empty());
    assert!(stop_requests(&cx, &root, child.id()) == 0);
    assert!(is_live(&child), "the detached child keeps running");
    assert!(is_live(&session_run));
}

#[test]
fn term_and_hup_carry_their_signal_as_the_cause() {
    assert_eq!(OwnerLoss::Signal(15).cause(), StopCause::Signal(15));
    assert_eq!(OwnerLoss::Signal(1).cause(), StopCause::Signal(1));
    assert_eq!(OwnerLoss::Disconnected.cause(), StopCause::OwnerLost);
    assert_eq!(OwnerLoss::EndOfInput.cause(), StopCause::OwnerLost);

    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");
    let tui_id = tui.id().clone();
    let (child, _) = admit_child_for(&cx, &root, &session, &mut session_run, 231, "dev");
    record_owned(&cx, &root, &session, &tui_id, child.id()).expect("owned");

    let end = authority
        .owner_lost(&cx, &root, &tui_id, OwnerLoss::Signal(15))
        .expect("term");
    assert_eq!(end.cause(), StopCause::Signal(15));
    assert_eq!(end.owned().to_vec(), vec![child.id()]);
    assert!(is_live(&session_run));
}

#[test]
fn an_owning_terminals_death_stops_its_runs_and_not_the_session() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");
    let tui_id = tui.id().clone();
    let (child, _) = admit_child_for(&cx, &root, &session, &mut session_run, 241, "dev");
    record_owned(&cx, &root, &session, &tui_id, child.id()).expect("owned");

    // The owning terminal dies: its handle is dropped without a detach.
    drop(tui);
    let ended = authority.reap(&cx, &root).expect("reap");
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].terminal(), &tui_id);
    assert!(!ended[0].released());
    assert_eq!(ended[0].owned().to_vec(), vec![child.id()]);
    assert_eq!(ended[0].cause(), StopCause::OwnerLost);

    let handle = SessionHandle::new(session.clone());
    handle
        .stop_owned(
            &cx,
            &root,
            &read_journal(&cx, &root),
            &RequestId::generate([242; REQUEST_ID_BYTES]),
            child.id(),
        )
        .expect("stop the dead terminal's run");
    assert_eq!(stop_requests(&cx, &root, child.id()), 1);
    assert!(is_live(&session_run), "the session survives its terminal");
}

#[test]
fn owned_runs_survive_a_supervisor_restart() {
    let cx = context();
    let fixture = home();
    let root = StateRoot::effects(&cx, fixture.path()).expect("state root");
    let (_, fence) = active_environment(&cx, &root);
    let (mut session_run, session) = admit_session(&cx, &root, &fence);
    let (mut authority, grant) = authority(&session);
    let tui = authority
        .attach(&cx, &root, &grant, TerminalKind::Tui, terminal(1), None)
        .expect("attach");
    let tui_id = tui.id().clone();
    let (child, _) = admit_child_for(&cx, &root, &session, &mut session_run, 251, "dev");
    record_owned(&cx, &root, &session, &tui_id, child.id()).expect("owned");

    // What a restarted supervisor reads: the durable record of who owned what,
    // and the same terminal attachment.
    let reader = read_journal(&cx, &root);
    match owned(&reader, &session, &tui_id) {
        Observed::Verified(runs, _) => assert_eq!(runs, vec![child.id()]),
        other => panic!("owned runs are not readable: {other:?}"),
    }
    assert_eq!(
        SessionRef::resolve(&reader, &environment())
            .verified()
            .cloned()
            .flatten(),
        Some(session.clone())
    );
    // The session is owned by its environment, never by a terminal.
    assert!(record_owned(&cx, &root, &session, &tui_id, session.run()).is_err());
}
