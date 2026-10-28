//! §8.11 proofs: replay-safe admission and secondary attachments against the
//! journal, its gate and the strict fake. No workload runs here; the named
//! kill boundaries re-execute this test binary and are the only children.
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
#[cfg(feature = "fault-injection")]
use std::path::{Path, PathBuf};
#[cfg(feature = "fault-injection")]
use std::process::{Command, Stdio};

#[cfg(feature = "fault-injection")]
const CHILD_HOME: &str = "EFFIGY_WORKSPACE_TEST_HOME";
#[cfg(feature = "fault-injection")]
const CHILD_STEP: &str = "EFFIGY_WORKSPACE_TEST_STEP";
#[cfg(feature = "fault-injection")]
const CHILD: &str = "run::workspace::tests::helper_workspace_child";

fn observe() -> ObserveContext {
    context()
        .operation()
        .observe()
        .expect("observation context")
}
fn admission_key(session: u64, request: u8) -> AdmissionKey {
    AdmissionKey {
        session: Generation::new(session),
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
fn state_of(cx: &EffectContext, root: &StateRoot) -> Option<String> {
    reader(cx, root)
        .conn()
        .query_row("SELECT state FROM workspace_admissions", [], |row| {
            row.get(0)
        })
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
fn workspace_admission_two_identical_requests_admit_exactly_one_run() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 1);
    let mut launches = Vec::new();

    // The first request wins the one reservation and may launch.
    let ticket = match reserve(&cx, &root, key, digest(), id(1)).expect("reserve") {
        Reservation::Fresh(ticket) => {
            launches.push((key, ticket.run()));
            ticket
        }
        other => panic!("expected the one reservation, saw {other:?}"),
    };
    // A concurrent identical request sees the reservation and launches nothing.
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Unknown {
            run: id(1),
            reason: UnknownAdmission::ReservedNotAdmitted
        }
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
    // A later retry with the same digest recovers the admitted run, whatever
    // run id it offered.
    assert_eq!(
        recorded(&cx, &root, key, id(2)),
        Recovered::Admitted {
            run: id(1),
            fence: run.fence().clone()
        }
    );
    // A retry that offers the run's own id is a replay too, not a reuse.
    assert_eq!(
        recorded(&cx, &root, key, id(1)),
        Recovered::Admitted {
            run: id(1),
            fence: run.fence().clone()
        }
    );
    assert_eq!(runs(&cx, &root), 1, "a key admits one run, not two");
    assert_eq!(state_of(&cx, &root).as_deref(), Some("admitted"));
    assert!(duplicate_launches(&launches).is_empty());

    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");
    match lookup_now(&cx, &root, key) {
        Observed::Verified(Recovered::Completed { run, outcome }, _) => {
            assert_eq!(run, id(1));
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
        reserve(&cx, &root, admission_key(1, 11), digest(), id(1)).is_err(),
        "a settled run id is not a fresh admission"
    );
}

#[test]
fn workspace_admission_duplicate_oracle_fails_on_a_broken_rebind() {
    let key = admission_key(1, 1);
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
fn workspace_admission_changed_digest_conflicts_and_never_readmits() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 2);
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
    let key = admission_key(1, 3);
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
    let owner = resume_owner(&cx, &root, key.session, id(1)).expect("owner reconnect");
    assert_eq!(owner.run(), id(1));
    assert_eq!(runs(&cx, &root), 1);
}

#[cfg(feature = "fault-injection")]
#[test]
fn workspace_admission_kill_boundaries_recover_without_a_second_run() {
    // Every named boundary is a real SIGKILL in a re-executed child.
    fn boundary(step: &str, point: Point) -> effigy_test_support::Fixture {
        let fixture = home();
        let status = run_child(fixture.path(), step, point);
        assert_eq!(
            status.signal(),
            Some(libc::SIGKILL),
            "{step} at {point:?} was not killed"
        );
        fixture
    }
    let cx = context();

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
    assert!(matches!(
        lookup_now(&cx, &root, admission_key(1, 1)),
        Observed::Absent(_)
    ));
    settled(&cx, &root, admission_key(1, 1), id(1), &mut fake);
    assert_eq!(runs(&cx, &root), 1);
    assert!(matches!(
        lookup_now(&cx, &root, admission_key(1, 1)),
        Observed::Verified(Recovered::Completed { .. }, _)
    ));

    // Between the reservation and the run commit: the reservation is durable
    // and is never readmitted.
    let second = boundary("reserve", Point::AdmissionReserveAfter);
    let home = second.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    assert_eq!(runs(&cx, &root), 0);
    assert_eq!(state_of(&cx, &root).as_deref(), Some("reserved"));
    match lookup_now(&cx, &root, admission_key(1, 1)) {
        Observed::Verified(
            Recovered::Unknown {
                run,
                reason: UnknownAdmission::ReservedNotAdmitted,
            },
            _,
        ) => assert_eq!(run, id(1)),
        other => panic!("reserved-but-unadmitted lookup {other:?}"),
    }
    assert_eq!(
        recorded(&cx, &root, admission_key(1, 1), id(1)),
        Recovered::Unknown {
            run: id(1),
            reason: UnknownAdmission::ReservedNotAdmitted
        }
    );
    assert_eq!(runs(&cx, &root), 0, "an unknown reservation launched a run");

    // After the admission commit: the run is recovered, never admitted again.
    let third = boundary("admit", Point::BeginAdmitted);
    let home = third.path().to_path_buf();
    let root = StateRoot::effects(&cx, &home).expect("root");
    assert_eq!(runs(&cx, &root), 1);
    let fence = match lookup_now(&cx, &root, admission_key(1, 1)) {
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
        recorded(&cx, &root, admission_key(1, 1), id(2)),
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
    assert_eq!(state_of(&cx, &root).as_deref(), Some("admitted"));
    assert!(matches!(
        lookup_now(&cx, &root, admission_key(1, 1)),
        Observed::Verified(Recovered::Admitted { .. }, _)
    ));
    // The next run on this owner settles the interrupted one, and that
    // recorded outcome is the answer for the key from then on.
    let successor = admitted(&cx, &root, admission_key(1, 2), id(2), &mut fake);
    match lookup_now(&cx, &root, admission_key(1, 1)) {
        Observed::Verified(Recovered::Completed { run, outcome }, _) => {
            assert_eq!(run, id(1));
            assert_eq!(outcome.workload, WorkloadOutcome::Succeeded);
        }
        other => panic!("outcome after recovery {other:?}"),
    }
    drop(successor);
}

#[test]
fn workspace_attachment_second_terminal_has_its_own_lifetime() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 4);
    let run = admitted(&cx, &root, key, id(1), &mut fake);
    let phase = run.phase().expect("phase");
    let mut owner = resume_owner(&cx, &root, key.session, id(1)).expect("owner");
    let mut first = attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([1; 16]))
        .expect("first terminal");
    let mut second = attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([2; 16]))
        .expect("second terminal");
    assert_eq!(first.client(), Client::Secondary);
    assert_eq!(owner.client(), Client::Owner);
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
    let key = admission_key(1, 5);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);

    // EOF: the client went away without acknowledging a detach.
    let mut owner = resume_owner(&cx, &root, key.session, id(1)).expect("owner");
    owner.release(&cx, &root, Release::Eof).expect("owner eof");
    assert_eq!(
        scalar(
            &cx,
            &root,
            "SELECT count(*) FROM run_attachments WHERE release='eof'"
        ),
        1
    );
    // An owner that released cannot take its run back, and the release alone
    // did not settle the run.
    assert!(resume_owner(&cx, &root, key.session, id(1)).is_err());
    assert_eq!(run.phase().expect("phase"), Phase::Admitted);

    run.finish(WorkloadOutcome::Succeeded).expect("finish");
    run.settle(&cleanup(&cx), &mut fake).expect("settle");

    // A detach is acknowledged before the client goes, and recorded apart.
    let key = admission_key(1, 6);
    let mut run = admitted(&cx, &root, key, id(2), &mut fake);
    let mut owner = resume_owner(&cx, &root, key.session, id(2)).expect("owner");
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
    assert!(resume_owner(&cx, &root, key.session, id(2)).is_err());
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
    let key = admission_key(1, 7);
    let run = admitted(&cx, &root, key, id(1), &mut fake);

    // Another session generation may not name this run.
    assert!(attach_secondary(
        &cx,
        &root,
        Generation::new(2),
        id(1),
        AttachmentId::new([9; 16])
    )
    .is_err());
    assert!(resume_owner(&cx, &root, Generation::new(2), id(1)).is_err());
    // A run the workspace channel never admitted is not attachable either: a
    // known run id is not authority.
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
    assert!(attach_secondary(&cx, &root, key.session, id(2), AttachmentId::new([9; 16])).is_err());
    // A run id that names nothing is refused, never created.
    assert!(resume_owner(&cx, &root, key.session, id(3)).is_err());
    assert!(attach_secondary(&cx, &root, key.session, id(3), AttachmentId::new([9; 16])).is_err());
    assert_eq!(runs(&cx, &root), 2);
}

#[test]
fn workspace_attachment_bounds_terminals_and_control_requests() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 8);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);
    let mut terminals = Vec::new();
    for n in 0..SECONDARY_MAX {
        let mut bytes = [0u8; 16];
        bytes[0] = n as u8;
        terminals.push(
            attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new(bytes))
                .expect("secondary"),
        );
    }
    assert!(
        attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([63; 16])).is_err(),
        "a run holds at most {SECONDARY_MAX} terminals"
    );
    // The same id twice is a conflict, not a second handle.
    assert!(attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([0; 16])).is_err());
    let terminal = terminals.remove(0);

    // Control is bounded while the run is live.
    for n in 0..CONTROL_MAX {
        terminal
            .control(&cx, &root, id(20 + n as u8), RequestAction::Stop)
            .expect("control");
    }
    assert!(terminal
        .control(&cx, &root, id(19), RequestAction::Stop)
        .is_err());
    assert_eq!(
        scalar(&cx, &root, "SELECT count(*) FROM requests"),
        CONTROL_MAX as i64
    );
    // A detach or settlement is not a secondary terminal's control.
    let mut second = terminals.remove(0);
    assert!(second
        .control(&cx, &root, id(30), RequestAction::Detach)
        .is_err());
    assert!(second
        .control(&cx, &root, id(30), RequestAction::Settle)
        .is_err());
    second
        .release(&cx, &root, Release::Detach)
        .expect("release");
    assert!(second
        .control(&cx, &root, id(31), RequestAction::Stop)
        .is_err());
}

#[test]
fn workspace_attachment_cancellation_settles_the_run_once() {
    let fixture = home();
    let home = fixture.path();
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 9);
    let mut run = admitted(&cx, &root, key, id(1), &mut fake);
    let terminal = attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([4; 16]))
        .expect("secondary");
    terminal
        .control(&cx, &root, id(20), RequestAction::Stop)
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
        .control(&cx, &root, id(21), RequestAction::Stop)
        .is_err());
    assert_eq!(state_of(&cx, &root).as_deref(), Some("closed"));
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
    let key = admission_key(1, 10);
    let _run = admitted(&cx, &root, key, id(1), &mut fake);
    let mut terminal = attach_secondary(&cx, &root, key.session, id(1), AttachmentId::new([5; 16]))
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

#[cfg(feature = "fault-injection")]
/// Re-executes this binary as `helper_workspace_child`, armed at one crossing.
fn run_child(home: &Path, step: &str, point: Point) -> std::process::ExitStatus {
    let mut command = Command::new(std::env::current_exe().expect("test binary"));
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
        .env(
            "EFFIGY_KERNEL_TEST_FAULT",
            fault::env_spec(point, 0, Fault::Kill),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    command.status().expect("child status")
}

#[cfg(feature = "fault-injection")]
/// The child runs the dispatcher's shape from reservation to settle, so the
/// armed fault lands on the boundary the parent named.
fn child_step(home: &Path, step: &str) -> Result<(), RunError> {
    let cx = context();
    let root = StateRoot::effects(&cx, home).expect("root");
    let mut fake = Fake::new(home);
    let key = admission_key(1, 1);
    match step {
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
        other => panic!("unknown step {other}"),
    }
    Ok(())
}

#[cfg(feature = "fault-injection")]
#[test]
#[ignore = "helper process"]
fn helper_workspace_child() {
    let Ok(home) = std::env::var(CHILD_HOME) else {
        return;
    };
    let home = PathBuf::from(home);
    let step = std::env::var(CHILD_STEP).expect("step");
    fault::arm_from_env();
    let _ = child_step(&home, &step);
    fault::arm(None);
}
