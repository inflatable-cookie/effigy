//! §8.11 proofs: replay-safe admission and secondary attachments against the
//! journal, its gate and the strict fake. The only children are the named kill
//! boundaries and the reservation handoff, each a re-execution of this binary.
use super::*;
use crate::ctx::*;
use crate::custody::Stream;
#[cfg(feature = "fault-injection")]
use crate::fault::{self, Fault, Point};
use crate::fs::{FileName, Trust};
use crate::id::*;
use crate::observe::*;
use crate::outcome::WorkloadOutcome;
use crate::root::{Family, StateRoot};
use crate::run::tests::{cleanup, context, home, id, inv, next, owner, Fake};
use crate::run::Run;
use crate::store::{Store, StoreReader};
use rusqlite::OptionalExtension;
use std::collections::BTreeSet;
#[cfg(feature = "fault-injection")]
use std::os::unix::process::ExitStatusExt;
use std::sync::{Arc, Barrier};

const CHILD_HOME: &str = "EFFIGY_WORKSPACE_TEST_HOME";
const CHILD_STEP: &str = "EFFIGY_WORKSPACE_TEST_STEP";
const CHILD_TICKET: &str = "EFFIGY_WORKSPACE_TEST_TICKET";
const CHILD: &str = "run::workspace::tests::helper_workspace_child";

fn observe() -> ObserveContext {
    context()
        .operation()
        .observe()
        .expect("observation context")
}
/// One workspace session: its own run and generation.
fn session() -> Session {
    Session {
        run: id(200),
        generation: Generation::new(1),
    }
}
fn admission_key(request: u8) -> AdmissionKey {
    AdmissionKey {
        session: session(),
        request: RequestId::new([request; 16]),
    }
}
fn digest() -> Digest {
    Digest::of(b"effigy.workspace.test.invoke.v1")
}
fn other_digest() -> Digest {
    Digest::of(b"effigy.workspace.test.invoke.v2")
}
/// A second task owner, so a run outside the workspace channel can begin while
/// the admitted run still holds its own owner gate.
fn foreign_owner() -> ResourceId {
    TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [7; 16]),
        "",
        Surface::Published,
        "foreign-task",
        None,
    )
    .expect("task id")
    .resource()
}
fn begin_opts(run: RunId) -> BeginOpts {
    BeginOpts {
        id: run,
        wait: Wait::FailFast,
        acknowledge: vec![],
        next: next(),
        custody: None,
    }
}
fn fresh(cx: &EffectContext, root: &StateRoot, key: AdmissionKey, run: RunId) -> AdmissionTicket {
    match reserve(cx, root, key, digest(), run).expect("reserve") {
        Reservation::Fresh(ticket) => ticket,
        other => panic!("expected the one reservation, saw {other:?}"),
    }
}
fn recorded(cx: &EffectContext, root: &StateRoot, key: AdmissionKey, run: RunId) -> Recovered {
    match reserve(cx, root, key, digest(), run).expect("reserve") {
        Reservation::Recorded(recovered) => recovered,
        Reservation::Fresh(_) => panic!("a second reservation was handed out"),
    }
}
fn reader(_cx: &EffectContext, root: &StateRoot) -> StoreReader {
    match Store::read(&observe(), root) {
        Observed::Verified(reader, _) => reader,
        other => panic!("store reader {other:?}"),
    }
}
fn scalar(cx: &EffectContext, root: &StateRoot, sql: &str) -> i64 {
    reader(cx, root)
        .conn()
        .query_row(sql, [], |row| row.get(0))
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}
fn runs(cx: &EffectContext, root: &StateRoot) -> i64 {
    scalar(cx, root, "SELECT count(*) FROM runs")
}
fn state_of(cx: &EffectContext, root: &StateRoot, key: AdmissionKey) -> Option<String> {
    reader(cx, root)
        .conn()
        .query_row(
            "SELECT state FROM workspace_admissions WHERE session_id=?1 AND session_generation=?2 AND request_id=?3",
            rusqlite::params![
                key.session.run.to_string(),
                key.session.generation.get() as i64,
                key.request.to_string()
            ],
            |row| row.get(0),
        )
        .optional()
        .unwrap()
}
fn lookup_now(cx: &EffectContext, root: &StateRoot, key: AdmissionKey) -> Observed<Recovered> {
    lookup(&reader(cx, root), key, digest())
}
fn admitted<'a>(
    cx: &EffectContext,
    root: &'a StateRoot,
    key: AdmissionKey,
    run: RunId,
    fake: &mut Fake,
) -> Run<'a> {
    let ticket = fresh(cx, root, key, run);
    Run::begin_admitted(
        cx,
        root,
        &inv(owner()),
        begin_opts(run),
        &cleanup(cx),
        fake,
        ticket,
    )
    .expect("admitted run")
}
fn settled(cx: &EffectContext, root: &StateRoot, key: AdmissionKey, run: RunId, fake: &mut Fake) {
    let mut run = admitted(cx, root, key, run, fake);
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(cx), fake).expect("settle");
}
/// The duplicate-admission oracle: one launch per key. A second launch for a
/// key is exactly the failure this module exists to prevent.
fn duplicate_launches(launches: &[(AdmissionKey, RunId)]) -> Vec<AdmissionKey> {
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for (key, _) in launches {
        if !seen.insert(*key) {
            duplicates.push(*key);
        }
    }
    duplicates
}

#[test]
fn workspace_admission_two_concurrent_identical_requests_admit_exactly_one_run() {
    let fixture = home();
    let home = fixture.path().to_path_buf();
    let key = admission_key(1);
    // Create the journal before the race: this test is about the reservation,
    // not about two processes creating a brand-new store at once.
    {
        let warm = context();
        let root = StateRoot::effects(&warm, &home).expect("root");
        let _ = reserve(&warm, &root, admission_key(99), digest(), id(99)).expect("warm store");
    }
    // Two sessions race the same key. The store's one writer, not a caller's
    // check, decides which of them may launch.
    let barrier = Arc::new(Barrier::new(2));
    let offered = |n: u8| id(10 + n);
    let results: Vec<Reservation> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2u8)
            .map(|n| {
                let home = home.clone();
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let cx = context();
                    let root = StateRoot::effects(&cx, &home).expect("root");
                    barrier.wait();
                    reserve(&cx, &root, key, digest(), offered(n)).expect("reserve")
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("racing reservation"))
            .collect()
    });
    let mut launches = Vec::new();
    let mut tickets = Vec::new();
    for result in results {
        match result {
            Reservation::Fresh(ticket) => {
                launches.push((key, ticket.run()));
                tickets.push(ticket);
            }
            // The loser never launches and is told it cannot establish an
            // admission, never that a run exists.
            Reservation::Recorded(Recovered::Unknown {
                reason: UnknownAdmission::ReservedNotAdmitted,
            }) => {}
            other => panic!("a concurrent identical request saw {other:?}"),
        }
    }
    assert_eq!(tickets.len(), 1, "exactly one caller may launch");
    assert!(duplicate_launches(&launches).is_empty());

    let ticket = tickets.into_iter().next().expect("the one reservation");
    let run_id = ticket.run();
    let cx = context();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let mut fake = Fake::new(&home);
    let mut run = Run::begin_admitted(
        &cx,
        &root,
        &inv(owner()),
        begin_opts(run_id),
        &cleanup(&cx),
        &mut fake,
        ticket,
    )
    .expect("admitted run");
    // Every later replay recovers the same run, whatever run id it offers.
    assert_eq!(
        recorded(&cx, &root, key, offered(0)),
        Recovered::Admitted {
            run: run_id,
            fence: run.fence().clone()
        }
    );
    assert_eq!(
        recorded(&cx, &root, key, offered(1)),
        Recovered::Admitted {
            run: run_id,
            fence: run.fence().clone()
        }
    );
    assert_eq!(runs(&cx, &root), 1, "a key admits one run, not two");
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("admitted"));

    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { run, outcome }, _) => {
            assert_eq!(run, run_id);
            assert_eq!(outcome.workload, WorkloadOutcome::Succeeded);
            assert!(outcome.cleanup_complete);
        }
        other => panic!("completed admission {other:?}"),
    }
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM run_attachments"),
        0
    );
    // A new key never reuses a run identity the journal already holds.
    assert!(
        reserve(&cx, &root, admission_key(11), digest(), run_id).is_err(),
        "a settled run id is not a fresh admission"
    );
}

#[test]
fn workspace_admission_duplicate_oracle_fails_on_a_broken_rebind() {
    let key = admission_key(1);
    let clean = vec![(key, id(1))];
    assert!(duplicate_launches(&clean).is_empty());
    // Reversible break probe: a rebind that admits the same key twice must
    // trip the oracle this module's duplicate-admission proof depends on.
    let mut broken = clean.clone();
    broken.push((key, id(2)));
    assert_eq!(duplicate_launches(&broken), vec![key]);
    broken.pop();
    assert!(duplicate_launches(&broken).is_empty());
}

#[test]
fn workspace_admission_a_reserved_request_never_names_a_run() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    // A run that is in the journal, but belongs to another key.
    settled(&cx, &root, admission_key(21), id(1), &mut fake);
    // A reservation for a different run id that never reached its admission.
    let key = admission_key(22);
    let ticket = fresh(&cx, &root, key, id(2));
    assert_eq!(ticket.run(), id(2));
    // The unadmitted reservation answers `Unknown` and names no run at all:
    // claiming id(2), or the unrelated live id(1), would attribute an
    // admission that never happened.
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Unknown { reason }, _) => {
            assert_eq!(reason, UnknownAdmission::ReservedNotAdmitted);
        }
        other => panic!("unadmitted reservation {other:?}"),
    }
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Unknown {
            reason: UnknownAdmission::ReservedNotAdmitted
        }
    );
    assert_eq!(runs(&cx, &root), 1, "a reservation admits no run");
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("reserved"));
}

#[test]
fn workspace_admission_one_run_id_belongs_to_one_key() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let first = admission_key(31);
    // A different session *and* a different request: the strongest collision.
    let second = AdmissionKey {
        session: Session {
            run: id(202),
            generation: Generation::new(1),
        },
        request: RequestId::new([32; 16]),
    };
    // A concurrent collision on distinct keys still grants only one ticket.
    drop(Store::open(&cx, &root).unwrap());
    let barrier = Barrier::new(2);
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = [first, second]
            .into_iter()
            .map(|key| {
                let barrier = &barrier;
                scope.spawn(move || {
                    let cx = context();
                    let root = StateRoot::effects(&cx, home).unwrap();
                    barrier.wait();
                    (key, reserve(&cx, &root, key, digest(), id(1)))
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    let mut winner = None;
    let mut loser = None;
    for (key, result) in results {
        match result {
            Ok(Reservation::Fresh(ticket)) => {
                assert!(
                    winner.replace((key, ticket)).is_none(),
                    "two keys reserved the same run"
                );
            }
            Err(RunError::Refused(rule)) if rule.contains("another workspace request") => {
                loser = Some(key);
            }
            other => panic!("colliding admission {other:?}"),
        }
    }
    let (first, ticket) = winner.expect("one reservation wins");
    let second = loser.expect("the other reservation is refused");
    // Before the admission commits, and after it does, the run identity is
    // this key's alone: the second key is refused by name and records nothing.
    let collision =
        |cx: &EffectContext, root: &StateRoot| match reserve(cx, root, second, digest(), id(1)) {
            Err(RunError::Refused(rule)) => {
                assert!(rule.contains("another workspace request"), "rule: {rule}")
            }
            other => panic!("a second key took the run identity: {other:?}"),
        };
    collision(&cx, &root);
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM workspace_admissions"),
        1
    );
    let mut run = Run::begin_admitted(
        &cx,
        &root,
        &inv(owner()),
        begin_opts(id(1)),
        &cleanup(&cx),
        &mut fake,
        ticket,
    )
    .expect("admitted run");
    collision(&cx, &root);
    assert!(matches!(
        lookup_now(&cx, &root, second),
        Observed::Absent(_)
    ));

    // The run reports the session that admitted it, so no attachment can be
    // attributed to the session that lost the collision.
    let owner_attachment = resume_owner(&cx, &root, first.session, id(1)).expect("owner");
    assert_eq!(owner_attachment.session(), first.session);
    assert!(resume_owner(&cx, &root, second.session, id(1)).is_err());
    assert!(attach_secondary(
        &cx,
        &root,
        second.session,
        id(1),
        AttachmentId::new([7; 16])
    )
    .is_err());
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM run_attachments"),
        1
    );
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    assert_eq!(runs(&cx, &root), 1);
}

#[test]
fn workspace_admission_a_reserved_run_id_is_not_an_ordinary_run() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(41);
    let mut ticket_holder = fresh(&cx, &root, key, id(1));

    // The reserved identity is the key's: an ordinary run that offers it is
    // refused inside its own admission transaction.
    match Run::begin(
        &cx,
        &root,
        &inv(owner()),
        begin_opts(id(1)),
        &cleanup(&cx),
        &mut fake,
    ) {
        Err(RunError::Refused(rule)) => {
            assert!(
                rule.contains("reserved for a keyed workspace request"),
                "{rule}"
            )
        }
        other => panic!("an ordinary run took a reserved id: {other:?}"),
    }
    assert_eq!(runs(&cx, &root), 0);
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("reserved"));
    // Nothing can attach to it, and the key still answers Unknown.
    assert!(resume_owner(&cx, &root, session(), id(1)).is_err());
    assert!(attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([8; 16])).is_err());
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Unknown { reason }, _) => {
            assert_eq!(reason, UnknownAdmission::ReservedNotAdmitted)
        }
        other => panic!("reserved key {other:?}"),
    }

    // The key's own admission still works, and now the identity is admitted.
    let run = Run::begin_admitted(
        &cx,
        &root,
        &inv(owner()),
        begin_opts(id(1)),
        &cleanup(&cx),
        &mut fake,
        ticket_holder,
    )
    .expect("admitted run");
    assert_eq!(run.fence().generation, Generation::new(1));
    ticket_holder = fresh(&cx, &root, admission_key(42), id(2));
    assert_eq!(ticket_holder.run(), id(2));
}

#[test]
fn workspace_admission_close_never_claims_a_reserved_row() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    // An ordinary run whose id a stale reservation also names. The admission
    // path now refuses that interleaving, so the row is written straight into
    // the journal: this checks the close side of the same rule on a store that
    // predates it.
    let mut plain = Run::begin(
        &cx,
        &root,
        &inv(foreign_owner()),
        begin_opts(id(2)),
        &cleanup(&cx),
        &mut fake,
    )
    .expect("plain run");
    let key = admission_key(43);
    {
        let mut store =
            Store::open(&cx.child(std::time::Duration::from_secs(30)), &root).expect("store");
        store
            .transaction(
                &cx.child(std::time::Duration::from_secs(30)),
                |txn| {
                    txn.conn().execute(
                        "INSERT INTO workspace_admissions(session_id,session_generation,request_id,digest,run_id,state,created_at,updated_at) \
                         VALUES (?1,?2,?3,?4,?5,'reserved',?6,?6)",
                        rusqlite::params![
                            session().run.to_string(),
                            session().generation.get() as i64,
                            key.request.to_string(),
                            digest().0.as_slice(),
                            id(2).to_string(),
                            txn.now()
                        ],
                    )?;
                    Ok::<_, RunError>(())
                },
            )
            .expect("stale reservation");
    }
    assert_eq!(
        admission_session(reader(&cx, &root).conn(), id(2)).unwrap(),
        None
    );
    assert!(attach_secondary(&cx, &root, key.session, id(2), AttachmentId::new([9; 16])).is_err());
    plain.finish(WorkloadOutcome::Succeeded).expect("finish");
    plain.settle(&cleanup(&cx), &mut fake).expect("settle");
    // The stale reservation names a closed run, but it never admitted one, so
    // it keeps answering Unknown instead of claiming that workload's outcome.
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("reserved"));
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Unknown { reason }, _) => {
            assert_eq!(reason, UnknownAdmission::ReservedNotAdmitted)
        }
        other => panic!("stale reservation {other:?}"),
    }
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM workspace_admissions WHERE state='closed'"
        ),
        0
    );
}

#[test]
fn workspace_admission_retry_classifies_before_the_run_id_check() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(46);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);
    // Another key already holds a different run identity.
    let other_key = admission_key(47);
    let other_ticket = fresh(&cx, &root, other_key, id(2));
    assert_eq!(other_ticket.run(), id(2));

    // A retry of the recorded key recovers its admission even though the id it
    // proposes now belongs to another key, and a changed digest still conflicts.
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Admitted {
            run: id(1),
            fence: run.fence().clone()
        }
    );
    match reserve(&cx, &root, key, other_digest(), id(2)).expect("reserve") {
        Reservation::Recorded(Recovered::Conflict { recorded }) => {
            assert_eq!(recorded, digest())
        }
        other => panic!("changed digest with a foreign id: {other:?}"),
    }
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
}

#[test]
fn workspace_attachment_release_fences_the_other_owner_handle() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(48);
    let run = admitted(&cx, &root, key, id(1), &mut fake);
    write_segment(&root, id(1), 1, Stream::Stdout, 0, b"kept");
    // Two handles on the same owning row: releasing one must fence the other,
    // which never saw that release in its own memory.
    let mut first = resume_owner(&cx, &root, session(), id(1)).expect("first owner");
    let mut second = resume_owner(&cx, &root, session(), id(1)).expect("second owner");
    first
        .release(&cx, &root, Release::Detach)
        .expect("owner detach");
    assert!(second
        .control(&cx, &root, id(80), RequestAction::Stop)
        .is_err());
    assert!(second
        .read(&observe(), &root, 1, Stream::Stdout, 4096)
        .is_err());
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM requests WHERE state='pending'"
        ),
        0,
        "a release must not be overtaken by a control"
    );
    assert_eq!(run.phase().expect("phase"), Phase::Admitted);
}

#[test]
#[cfg(feature = "fault-injection")]
fn workspace_attachment_release_wins_while_control_is_before_its_transaction() {
    let fixture = home();
    let cx = context();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let mut fake = Fake::new(fixture.path());
    let _run = admitted(&cx, &root, admission_key(49), id(1), &mut fake);
    let mut owner = resume_owner(&cx, &root, session(), id(1)).unwrap();
    let child = child_command(fixture.path(), "control-release-race")
        .env(
            "EFFIGY_KERNEL_TEST_FAULT",
            fault::env_spec(Point::RequestBefore, 0, Fault::Stall),
        )
        .spawn()
        .unwrap();
    let mut child = TestChild(child);
    let pid = child.0.id() as libc::pid_t;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let mut state = 0;
        // SAFETY: wait only for the exact child this test spawned.
        let waited = unsafe { libc::waitpid(pid, &mut state, libc::WNOHANG | libc::WUNTRACED) };
        if waited == pid {
            assert!(
                libc::WIFSTOPPED(state),
                "control child exited instead of stalling"
            );
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "control did not stall"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    owner.release(&cx, &root, Release::Detach).unwrap();
    // SAFETY: continue only this test's stopped child.
    assert_eq!(unsafe { libc::kill(pid, libc::SIGCONT) }, 0);
    assert!(child.0.wait().unwrap().success());
    assert_eq!(scalar(&cx, &root, "SELECT count(*) FROM requests"), 0);
}

struct TestChild(std::process::Child);
impl Drop for TestChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn workspace_attachment_control_respects_its_budget_while_store_is_held() {
    let fixture = home();
    let cx = context();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let mut fake = Fake::new(fixture.path());
    let run = admitted(&cx, &root, admission_key(50), id(1), &mut fake);
    let attachment = resume_owner(&cx, &root, session(), id(1)).unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let fixture_home = fixture.path().to_path_buf();
    let holder = std::thread::spawn(move || {
        let cx = context();
        let root = StateRoot::effects(&cx, &fixture_home).unwrap();
        let mut store = Store::open(&cx, &root).unwrap();
        store
            .transaction(&cx, |_| {
                ready_tx.send(()).unwrap();
                // A bounded backstop avoids trapping the runner if the control
                // mistakenly inherits settlement's thirty-second writer wait.
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                Ok::<_, RunError>(())
            })
            .unwrap();
    });
    ready_rx.recv().unwrap();
    let result = attachment.control(
        &cx.child(std::time::Duration::from_millis(100)),
        &root,
        id(89),
        RequestAction::Stop,
    );
    release_tx.send(()).unwrap();
    holder.join().unwrap();
    assert!(
        result.is_err(),
        "a held writer must not exceed the control budget"
    );
    assert!(run.requests().unwrap().is_empty());
    attachment
        .control(&cx, &root, id(90), RequestAction::Stop)
        .unwrap();
    assert_eq!(
        run.requests().unwrap().len(),
        1,
        "cancel remains serviceable after the held writer"
    );
}

#[test]
fn workspace_admission_changed_digest_conflicts_and_never_readmits() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(2);
    settled(&cx, &root, key, id(1), &mut fake);

    match lookup(&reader(&cx, &root), key, other_digest()) {
        Observed::Verified(Recovered::Conflict { recorded }, _) => assert_eq!(recorded, digest()),
        other => panic!("changed digest lookup {other:?}"),
    }
    assert_eq!(
        reserve(&cx, &root, key, other_digest(), id(2)).expect("reserve"),
        Reservation::Recorded(Recovered::Conflict { recorded: digest() })
    );
    assert_eq!(runs(&cx, &root), 1, "a conflict admits nothing");
}

#[test]
fn workspace_admission_reconnect_after_restart_recovers_the_same_run() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let key = admission_key(3);
    let fence = {
        let root = StateRoot::effects(&cx, home).expect("root");
        let mut fake = Fake::new(home);
        let run = admitted(&cx, &root, key, id(1), &mut fake);
        run.fence().clone()
    };
    // Every handle is gone: a supervisor restart reopens the journal.
    let root = StateRoot::effects(&cx, home).expect("reopened root");
    match lookup_now(&cx, &root, key) {
        Observed::Verified(
            Recovered::Admitted {
                run: recovered_run,
                fence: recovered_fence,
            },
            _,
        ) => {
            assert_eq!(recovered_run, id(1));
            assert_eq!(recovered_fence, fence);
        }
        other => panic!("reconnected admission {other:?}"),
    }
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Admitted { run: id(1), fence }
    );
    let owner = resume_owner(&cx, &root, session(), id(1)).expect("owner reconnect");
    assert_eq!(owner.run(), id(1));
    assert_eq!(owner.session(), session());
    assert_eq!(runs(&cx, &root), 1);
}

/// The reservation the host dispatcher holds is handed to a detached engine
/// supervisor in its config frame (see `supervise::EngineSession::admission`);
/// here the same ticket crosses a process boundary and admits there.
#[test]
fn workspace_admission_hands_a_reservation_to_another_process() {
    let fixture = home();
    let home = fixture.path().to_path_buf();
    let cx = context();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let key = admission_key(4);
    let ticket = fresh(&cx, &root, key, id(1));
    let encoded = serde_json::to_string(&ticket).expect("ticket frame");

    let mut child = child_command(&home, "detached-admit")
        .env(CHILD_TICKET, encoded)
        .spawn()
        .expect("handoff child");
    wait_for(&home.join("admitted"));
    // The other process admitted under the handed ticket.
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Admitted { run, .. }, _) => assert_eq!(run, id(1)),
        other => panic!("handed-off admission {other:?}"),
    }
    std::fs::write(home.join("release"), b"settle").expect("release");
    assert!(child.wait().expect("handoff child").success());
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { run, outcome }, _) => {
            assert_eq!(run, id(1));
            assert_eq!(outcome.workload, WorkloadOutcome::Succeeded);
        }
        other => panic!("handed-off outcome {other:?}"),
    }
    assert_eq!(runs(&cx, &root), 1);
}

fn wait_for(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[cfg(feature = "fault-injection")]
fn workspace_admission_kill_boundaries_recover_without_a_second_run() {
    // Every named boundary is a real SIGKILL in a re-executed child.
    fn boundary(step: &str, point: Point) -> effigy_test_support::Fixture {
        let fixture = home();
        let status = run_kill_child(fixture.path(), step, point);
        assert_eq!(
            status.signal(),
            Some(libc::SIGKILL),
            "{step} at {point:?} was not killed"
        );
        fixture
    }
    let cx = context();
    let key = admission_key(1);

    // Before the reservation: nothing was recorded, so admitting now is the
    // first admission.
    let first = boundary("reserve", Point::AdmissionReserveBefore);
    let home = first.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let mut fake = Fake::new(&home);
    assert_eq!(runs(&cx, &root), 0);
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM workspace_admissions"),
        0
    );
    assert!(matches!(lookup_now(&cx, &root, key), Observed::Absent(_)));
    settled(&cx, &root, key, id(1), &mut fake);
    assert_eq!(runs(&cx, &root), 1);
    assert!(matches!(
        lookup_now(&cx, &root, key),
        Observed::Verified(Recovered::Completed { .. }, _)
    ));

    // Between the reservation and the run commit: the reservation is durable
    // and is never readmitted.
    let second = boundary("reserve", Point::AdmissionReserveAfter);
    let home = second.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    assert_eq!(runs(&cx, &root), 0);
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("reserved"));
    match lookup_now(&cx, &root, key) {
        Observed::Verified(
            Recovered::Unknown {
                reason: UnknownAdmission::ReservedNotAdmitted,
            },
            _,
        ) => {}
        other => panic!("reserved-but-unadmitted lookup {other:?}"),
    }
    assert_eq!(
        recorded(&cx, &root, key, id(1)),
        Recovered::Unknown {
            reason: UnknownAdmission::ReservedNotAdmitted
        }
    );
    assert_eq!(runs(&cx, &root), 0, "an unknown reservation launched a run");

    // After the admission commit: the run is recovered, never admitted again.
    let third = boundary("admit", Point::BeginAdmitted);
    let home = third.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    assert_eq!(runs(&cx, &root), 1);
    let fence = match lookup_now(&cx, &root, key) {
        Observed::Verified(
            Recovered::Admitted {
                run: recovered_run,
                fence,
            },
            _,
        ) => {
            assert_eq!(recovered_run, id(1));
            fence
        }
        other => panic!("admission after commit {other:?}"),
    };
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Admitted { run: id(1), fence }
    );
    assert_eq!(runs(&cx, &root), 1);

    // Before the outcome: the run is live and unclosed, so the key answers
    // with its admission until the run's own settle records the outcome.
    let fourth = boundary("settle", Point::SettleCloseBefore);
    let home = fourth.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let mut fake = Fake::new(&home);
    assert_eq!(runs(&cx, &root), 1);
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("admitted"));
    assert!(matches!(
        lookup_now(&cx, &root, key),
        Observed::Verified(Recovered::Admitted { .. }, _)
    ));
    // The next run on this owner settles the interrupted one, and that
    // recorded outcome is the answer for the key from then on.
    let successor = admitted(&cx, &root, admission_key(2), id(2), &mut fake);
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { run, outcome }, _) => {
            assert_eq!(run, id(1));
            assert_eq!(outcome.workload, WorkloadOutcome::Succeeded);
        }
        other => panic!("outcome after recovery {other:?}"),
    }
    drop(successor);
}

#[test]
fn workspace_attachment_sessions_are_distinguished_by_run_and_generation() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(5);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);

    // Same generation, different session run: a different session.
    let other_run = Session {
        run: id(201),
        generation: session().generation,
    };
    // Same session run, next generation: a stale capability generation.
    let other_generation = Session {
        run: session().run,
        generation: Generation::new(2),
    };
    for other in [other_run, other_generation] {
        assert!(
            attach_secondary(&cx, &root, other, id(1), AttachmentId::new([9; 16])).is_err(),
            "session {other:?} attached another session's run"
        );
        assert!(
            resume_owner(&cx, &root, other, id(1)).is_err(),
            "session {other:?} took another session's owning client"
        );
    }
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM run_attachments"),
        1
    );
}

#[test]
fn workspace_attachment_second_terminal_has_its_own_lifetime() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(6);
    let run = admitted(&cx, &root, key, id(1), &mut fake);
    let phase = run.phase().expect("phase");
    let mut owner = resume_owner(&cx, &root, session(), id(1)).expect("owner");
    let mut first = attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([1; 16]))
        .expect("first terminal");
    let mut second = attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([2; 16]))
        .expect("second terminal");
    assert_eq!(first.client(), Client::Secondary);
    assert_eq!(owner.client(), Client::Owner);
    assert_eq!(first.session(), session());
    assert!(!first.released() && !second.released() && !owner.released());
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE state='attached'"
        ),
        3
    );

    first.release(&cx, &root, Release::Eof).expect("release");
    assert!(!first.attached(&cx, &root).expect("lifetime"));
    // The owner's lifetime and the run are untouched by a secondary going.
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE lifetime='owner' AND state='attached'"
        ),
        1
    );
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE state='attached'"
        ),
        2
    );
    assert_eq!(run.phase().expect("phase"), phase);
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM runs WHERE phase='closed'"),
        0
    );
    second.release(&cx, &root, Release::Detach).expect("detach");
    owner.release(&cx, &root, Release::Detach).expect("detach");
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE state='attached'"
        ),
        0
    );
}

#[test]
fn workspace_attachment_owner_eof_and_detach_are_recorded_apart() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(7);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);

    // EOF: the client went away without acknowledging a detach, so the handle
    // records the stop its run needs (§8.11) besides the release.
    let mut owner = resume_owner(&cx, &root, session(), id(1)).expect("owner");
    owner.release(&cx, &root, Release::Eof).expect("owner eof");
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE release='eof'"
        ),
        1
    );
    // An owner that released cannot take its run back.
    assert!(resume_owner(&cx, &root, session(), id(1)).is_err());
    let pending = run.requests().expect("requests");
    assert_eq!(pending.len(), 1, "owner EOF must record the run's stop");
    assert!(matches!(pending[0].action, RequestAction::Stop));
    assert_eq!(pending[0].run, id(1));

    // The owner loop drains that stop: the run is cancelled and settled once.
    run.resolve_request(&pending[0]).expect("resolve");
    run.interrupt(
        &cleanup(&cx),
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    )
    .expect("interrupt");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { outcome, .. }, _) => match outcome.workload {
            WorkloadOutcome::Cancelled { cause, .. } => assert_eq!(cause, StopCause::Requested),
            other => panic!("owner EOF outcome {other:?}"),
        },
        other => panic!("owner EOF settlement {other:?}"),
    }

    // A detach is acknowledged before the client goes, and records no stop: the
    // run keeps going.
    let key = admission_key(8);
    let mut run = admitted(&cx, &root, key, id(2), &mut fake);
    let mut owner = resume_owner(&cx, &root, session(), id(2)).expect("owner");
    owner
        .release(&cx, &root, Release::Detach)
        .expect("owner detach");
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE release='detach'"
        ),
        1
    );
    assert!(resume_owner(&cx, &root, session(), id(2)).is_err());
    assert!(
        run.requests().expect("requests").is_empty(),
        "an acknowledged detach must not stop the run"
    );
    assert_eq!(run.phase().expect("phase"), Phase::Admitted);
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    // A run's close ends every client lifetime it had.
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM run_attachments"),
        0
    );
    assert!(matches!(
        lookup_now(&cx, &root, key),
        Observed::Verified(Recovered::Completed { .. }, _)
    ));
}

#[test]
fn workspace_attachment_refuses_foreign_and_unknown_runs() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(9);
    let run = admitted(&cx, &root, key, id(1), &mut fake);

    // A run the workspace channel never admitted is not attachable: a known
    // run id is not authority.
    let plain = Run::begin(
        &cx,
        &root,
        &inv(foreign_owner()),
        begin_opts(id(2)),
        &cleanup(&cx),
        &mut fake,
    )
    .expect("plain run");
    assert_ne!(plain.fence(), run.fence());
    assert!(attach_secondary(&cx, &root, session(), id(2), AttachmentId::new([9; 16])).is_err());
    // A run id that names nothing is refused, never created.
    assert!(resume_owner(&cx, &root, session(), id(3)).is_err());
    assert!(attach_secondary(&cx, &root, session(), id(3), AttachmentId::new([9; 16])).is_err());
    assert_eq!(runs(&cx, &root), 2);
}

#[test]
fn workspace_attachment_lifetime_ends_with_the_run() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(10);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);
    write_segment(&root, id(1), 1, Stream::Stdout, 0, b"before-close");
    let mut terminal = attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([6; 16]))
        .expect("secondary");
    assert!(terminal.attached(&cx, &root).expect("lifetime"));
    terminal
        .read(&observe(), &root, 1, Stream::Stdout, 4096)
        .expect("read");

    // The run closes while this handle is still alive: its row goes with the
    // run, so the handle stops reading and steering even though it was never
    // released itself.
    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    assert!(!terminal.attached(&cx, &root).expect("lifetime"));
    assert!(terminal
        .read(&observe(), &root, 1, Stream::Stdout, 4096)
        .is_err());
    assert!(terminal
        .control(&cx, &root, id(40), RequestAction::Stop)
        .is_err());
}

#[test]
fn workspace_attachment_control_bound_holds_under_concurrency() {
    let fixture = home();
    let home = fixture.path().to_path_buf();
    let cx = context();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let mut fake = Fake::new(&home);
    let key = admission_key(11);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);
    let mut terminals = Vec::new();
    for n in 0..2u8 {
        terminals.push(
            attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([n; 16]))
                .expect("secondary"),
        );
    }
    // Two terminals each try a full bound of stops at the same time. The bound
    // and the insert commit together, so the run can never hold more than
    // CONTROL_MAX waiting requests.
    let barrier = Arc::new(Barrier::new(2));
    let accepted: Vec<usize> = std::thread::scope(|scope| {
        let handles: Vec<_> = terminals
            .into_iter()
            .enumerate()
            .map(|(index, terminal)| {
                let home = home.clone();
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    let cx = context();
                    let root = StateRoot::effects(&cx, &home).expect("root");
                    barrier.wait();
                    (0..CONTROL_MAX)
                        .filter(|n| {
                            terminal
                                .control(
                                    &cx,
                                    &root,
                                    id(40 + index as u8 * 10 + *n as u8),
                                    RequestAction::Stop,
                                )
                                .is_ok()
                        })
                        .count()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("racing control"))
            .collect()
    });
    assert_eq!(accepted.iter().sum::<usize>(), CONTROL_MAX);
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM requests WHERE state='pending'"
        ),
        CONTROL_MAX as i64
    );
}

#[test]
fn workspace_attachment_bounds_terminals_and_control_requests() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(12);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);
    let mut terminals = Vec::new();
    for n in 0..SECONDARY_MAX {
        let mut bytes = [0u8; 16];
        bytes[0] = n as u8;
        terminals.push(
            attach_secondary(&cx, &root, session(), id(1), AttachmentId::new(bytes))
                .expect("secondary"),
        );
    }
    assert!(
        attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([63; 16])).is_err(),
        "a run holds at most {SECONDARY_MAX} terminals"
    );
    // The same id twice is a conflict, not a second handle.
    assert!(attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([0; 16])).is_err());
    let terminal = terminals.remove(0);

    // Control is bounded while the run is live.
    for n in 0..CONTROL_MAX {
        terminal
            .control(&cx, &root, id(50 + n as u8), RequestAction::Stop)
            .expect("control");
    }
    assert!(terminal
        .control(&cx, &root, id(59), RequestAction::Stop)
        .is_err());
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM requests"),
        CONTROL_MAX as i64
    );
    // A detach or settlement is not a secondary terminal's control, and a
    // released terminal has no control path at all.
    let mut second = terminals.remove(0);
    assert!(second
        .control(&cx, &root, id(60), RequestAction::Detach)
        .is_err());
    assert!(second
        .control(&cx, &root, id(60), RequestAction::Settle)
        .is_err());
    second
        .release(&cx, &root, Release::Detach)
        .expect("release");
    assert!(second
        .control(&cx, &root, id(61), RequestAction::Stop)
        .is_err());
}

#[test]
fn workspace_attachment_owner_loss_preserves_the_full_control_bound() {
    let fixture = home();
    let cx = context();
    let root = StateRoot::effects(&cx, fixture.path()).unwrap();
    let mut fake = Fake::new(fixture.path());
    let run = admitted(&cx, &root, admission_key(51), id(1), &mut fake);
    let mut owner = resume_owner(&cx, &root, session(), id(1)).unwrap();
    for n in 0..CONTROL_MAX {
        owner
            .control(&cx, &root, id(60 + n as u8), RequestAction::Stop)
            .unwrap();
    }
    owner.release(&cx, &root, Release::Eof).unwrap();
    owner.release(&cx, &root, Release::Eof).unwrap();
    assert_eq!(run.requests().unwrap().len(), CONTROL_MAX);
    assert!(!owner.attached(&cx, &root).unwrap());
}

#[test]
fn workspace_attachment_cancellation_settles_the_run_once() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(13);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);
    let terminal = attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([4; 16]))
        .expect("secondary");
    terminal
        .control(&cx, &root, id(70), RequestAction::Stop)
        .expect("cancel");
    let pending = run.requests().expect("requests");
    assert_eq!(pending.len(), 1);
    run.resolve_request(&pending[0]).expect("resolve");
    run.interrupt(
        &cleanup(&cx),
        WorkloadOutcome::Cancelled {
            cause: StopCause::Requested,
            signal: None,
        },
    )
    .expect("interrupt");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { outcome, .. }, _) => match outcome.workload {
            WorkloadOutcome::Cancelled { cause, .. } => assert_eq!(cause, StopCause::Requested),
            other => panic!("cancelled outcome {other:?}"),
        },
        other => panic!("settled admission {other:?}"),
    }
    // Settlement happens once: control on the closed run is refused, and its
    // one key record is closed exactly once.
    assert!(terminal
        .control(&cx, &root, id(71), RequestAction::Stop)
        .is_err());
    assert_eq!(state_of(&cx, &root, key).as_deref(), Some("closed"));
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM workspace_admissions WHERE state='closed'"
        ),
        1
    );
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM runs WHERE phase='closed'"),
        1
    );
}

#[test]
fn workspace_attachment_cursor_reports_a_gap_and_refuses_an_invalid_position() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(14);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);
    let mut terminal = attach_secondary(&cx, &root, session(), id(1), AttachmentId::new([5; 16]))
        .expect("secondary");

    // Bytes 0..100 rotated away before this terminal attached.
    write_segment(&root, id(1), 1, Stream::Stdout, 100, b"hello");
    let chunk = terminal
        .read(&observe(), &root, 1, Stream::Stdout, 4096)
        .expect("read");
    assert_eq!(chunk.gap, Some(Gap { from: 0, to: 100 }));
    assert_eq!(chunk.bytes, b"hello");
    assert_eq!(chunk.from, 100);
    assert_eq!(chunk.next, 105);
    assert_eq!(terminal.cursor(1, Stream::Stdout), 105);
    let chunk = terminal
        .read(&observe(), &root, 1, Stream::Stdout, 4096)
        .expect("read");
    assert_eq!(chunk.gap, None);
    assert!(chunk.bytes.is_empty());

    // A bound the channel cannot frame is refused before any read.
    assert!(terminal
        .read(&observe(), &root, 1, Stream::Stdout, 0)
        .is_err());
    assert!(terminal
        .read(&observe(), &root, 1, Stream::Stdout, MAX_CHUNK + 1)
        .is_err());
    assert!(terminal
        .read(&observe(), &root, 9, Stream::Stdout, 16)
        .is_err());
    // A cursor ahead of what the log now holds is refused, never clamped.
    write_segment(&root, id(1), 1, Stream::Stdout, 100, b"abc");
    assert!(terminal
        .read(&observe(), &root, 1, Stream::Stdout, 16)
        .is_err());
    terminal.release(&cx, &root, Release::Eof).expect("release");
    assert!(terminal
        .read(&observe(), &root, 1, Stream::Stdout, 16)
        .is_err());
}

fn write_segment(
    root: &StateRoot,
    run: RunId,
    sequence: i64,
    stream: Stream,
    base: u64,
    bytes: &[u8],
) {
    use std::io::Write;
    let runs = root
        .family(Family::Runs)
        .expect("runs family")
        .expect("created family");
    let dir = runs
        .child(&run.to_string(), Trust::Private, true)
        .expect("run directory")
        .expect("created directory");
    let name = FileName::new(&format!("{sequence}.{}.{base}.log", stream.name())).expect("name");
    let (mut file, _) = dir
        .open_file(
            &name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            Some(0o600),
            Trust::Private,
        )
        .expect("segment")
        .expect("created segment");
    file.write_all(bytes).expect("segment bytes");
}

/// A re-execution of this binary as the helper, for the handoff and for the
/// kill boundaries.
fn child_command(home: &std::path::Path, step: &str) -> std::process::Command {
    let mut command = std::process::Command::new(std::env::current_exe().expect("test binary"));
    command
        .args([
            CHILD,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_HOME, home)
        .env(CHILD_STEP, step)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit());
    command
}

/// The helper runs the dispatcher's shape from reservation to settle, so the
/// armed fault lands on the boundary the parent named.
fn child_step(home: &std::path::Path, step: &str) -> Result<(), RunError> {
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1);
    match step {
        "control-release-race" => {
            let attachment = resume_owner(&cx, &root, session(), id(1))?;
            match attachment.control(&cx, &root, id(80), RequestAction::Stop) {
                Err(RunError::Refused(rule)) if rule.contains("no longer attached") => {}
                other => panic!("control after concurrent release: {other:?}"),
            }
        }
        "reserve" => {
            let _ = reserve(&cx, &root, key, digest(), id(1))?;
        }
        "admit" => {
            let ticket = fresh(&cx, &root, key, id(1));
            let _ = Run::begin_admitted(
                &cx,
                &root,
                &inv(owner()),
                begin_opts(id(1)),
                &cleanup(&cx),
                &mut fake,
                ticket,
            )?;
        }
        "settle" => {
            let ticket = fresh(&cx, &root, key, id(1));
            let mut run = Run::begin_admitted(
                &cx,
                &root,
                &inv(owner()),
                begin_opts(id(1)),
                &cleanup(&cx),
                &mut fake,
                ticket,
            )?;
            run.finish(WorkloadOutcome::Succeeded)?;
            run.settle(&cleanup(&cx), &mut fake)?;
        }
        "detached-admit" => {
            // What an engine supervisor's own process does with the ticket its
            // dispatcher handed it (052 §8.11, supervise::EngineLaunch).
            let frame = std::env::var(CHILD_TICKET).expect("ticket frame");
            let ticket: AdmissionTicket =
                serde_json::from_str(&frame).expect("ticket this store wrote");
            let run_id = ticket.run();
            let mut run = Run::begin_admitted(
                &cx,
                &root,
                &inv(owner()),
                begin_opts(run_id),
                &cleanup(&cx),
                &mut fake,
                ticket,
            )?;
            std::fs::write(home.join("admitted"), run_id.to_string())
                .map_err(|error| RunError::unavailable(format!("handoff marker ({error})")))?;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while !home.join("release").exists() {
                if std::time::Instant::now() > deadline {
                    return Err(RunError::unavailable("handoff release wait ran out"));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            run.finish(WorkloadOutcome::Succeeded)?;
            run.settle(&cleanup(&cx), &mut fake)?;
        }
        other => panic!("unknown step {other}"),
    }
    Ok(())
}

#[test]
#[ignore = "helper process"]
fn helper_workspace_child() {
    let Ok(home) = std::env::var(CHILD_HOME) else {
        return;
    };
    let home = std::path::PathBuf::from(home);
    let step = std::env::var(CHILD_STEP).expect("step");
    #[cfg(feature = "fault-injection")]
    crate::fault::arm_from_env();
    let result = child_step(&home, &step);
    #[cfg(feature = "fault-injection")]
    crate::fault::arm(None);
    if result.is_err() {
        std::process::exit(1);
    }
}

/// One kill boundary: the child is re-executed with a named fault that kills
/// its own process, so the parent recovers from the durable state it left.
#[cfg(feature = "fault-injection")]
fn run_kill_child(home: &std::path::Path, step: &str, point: Point) -> std::process::ExitStatus {
    child_command(home, step)
        .env(
            "EFFIGY_KERNEL_TEST_FAULT",
            fault::env_spec(point, 0, Fault::Kill),
        )
        .status()
        .expect("child status")
}
