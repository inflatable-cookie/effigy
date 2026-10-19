//! §12.3 supervisor matrix as cheap model tests: the full cross product of
//! front end × prior state × stop × attachment over the kernel primitives a
//! run supervisor uses. The workload is a strict-fake effect, not a process;
//! process custody under each stop has its own qualification suite.
use super::oracles::Oracle;
use super::scenarios::{
    active_environment, apply, begin, env_fence, id, settle, task, transition, Fake,
};
use super::world::{install_audits, World};
use super::*;
use crate::id::RunId;
use crate::outcome::{CleanupOutcome, WorkloadOutcome};
use crate::root::StateRoot;
use crate::run::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Front {
    Tui,
    AttachedText,
    Detached,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Prior {
    Fresh,
    Interrupted,
    ClosedWithResidue,
    Stopped,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Request,
    Int,
    Term,
    Hup,
    OwningClientKill,
    OwnerKill,
    Handover,
    Down,
}
const FRONTS: [Front; 3] = [Front::Tui, Front::AttachedText, Front::Detached];
const PRIORS: [Prior; 4] = [
    Prior::Fresh,
    Prior::Interrupted,
    Prior::ClosedWithResidue,
    Prior::Stopped,
];
const STOPS: [Stop; 8] = [
    Stop::Request,
    Stop::Int,
    Stop::Term,
    Stop::Hup,
    Stop::OwningClientKill,
    Stop::OwnerKill,
    Stop::Handover,
    Stop::Down,
];
const ATTACHMENTS: [Option<AttachMode>; 3] = [
    None,
    Some(AttachMode::Borrow),
    Some(AttachMode::Exclusive {
        stop_on_release: true,
    }),
];
const OWNER: u8 = 12;

fn requested(
    run: &mut Run<'_>,
    root: &StateRoot,
    action: RequestAction,
    n: u8,
) -> Result<(), RunError> {
    let cx = mutate();
    request(
        &cx,
        root,
        &Request {
            id: id(n),
            run: run.id(),
            expected: run.fence().clone(),
            action,
        },
    )?;
    for pending in run.requests()? {
        run.resolve_request(&pending)?;
    }
    Ok(())
}
fn cancelled(cause: StopCause) -> WorkloadOutcome {
    let signal = match cause {
        StopCause::Signal(signal) => Some(signal),
        _ => None,
    };
    WorkloadOutcome::Cancelled { cause, signal }
}
fn stop_by_request(run: &mut Run<'_>, root: &StateRoot, n: u8) {
    requested(run, root, RequestAction::Stop, n).expect("stop request");
    run.interrupt(&cleanup(&mutate()), cancelled(StopCause::Requested))
        .expect("interrupt");
}
fn status(home: &Path, run: RunId) -> Observed<RunStatus> {
    let cx = observe();
    let root = verified(StateRoot::observe(&cx, home));
    let reader = verified(crate::store::Store::read(&cx, &root));
    crate::run::status(&cx, &root, &reader, run, next())
}

/// One cell, from a fresh HOME to every attachment released exactly once.
fn cell(front: Front, prior: Prior, stop: Stop, attachment: Option<AttachMode>) {
    let label = format!("{front:?}/{prior:?}/{stop:?}/{attachment:?}");
    let dir = home();
    let home = dir.home();
    install_audits(&home);
    active_environment(&home);
    let cx = mutate();
    let root = StateRoot::effects(&cx, &home).expect("root");
    let mut fake = Fake::new(&home);
    let attach = |run: &mut Run<'_>| {
        if let Some(mode) = attachment {
            Attachments::attach(run, &env_fence(1), mode, crate::gate::Wait::FailFast)
                .expect("attach");
        }
    };

    // Prior state of the same owner.
    let mut acknowledge = vec![];
    if prior != Prior::Fresh {
        let mut previous =
            begin(&cx, &root, task(OWNER), 1, None, vec![], &mut fake).expect("prior");
        attach(&mut previous);
        let key = if prior == Prior::ClosedWithResidue {
            "stuck"
        } else {
            "prior"
        };
        apply(&mut previous, &fake, key).expect("prior effect");
        match prior {
            Prior::Interrupted => drop(previous),
            Prior::ClosedWithResidue => {
                previous.finish(WorkloadOutcome::Succeeded).expect("finish");
                let outcome = previous.settle(&cleanup(&cx), &mut fake).expect("settle");
                assert_eq!(outcome.cleanup(), CleanupOutcome::Incomplete, "{label}");
                // Residue blocks an unacknowledged successor (K5).
                assert!(
                    matches!(
                        begin(&cx, &root, task(OWNER), 2, None, vec![], &mut fake),
                        Err(RunError::PredecessorUnsettled { .. })
                    ),
                    "{label}: residue admitted a successor"
                );
                acknowledge.push(id(1));
            }
            Prior::Stopped => {
                previous
                    .interrupt(&cleanup(&cx), cancelled(StopCause::Requested))
                    .expect("interrupt");
                let outcome = previous.settle(&cleanup(&cx), &mut fake).expect("settle");
                assert_eq!(outcome.cleanup(), CleanupOutcome::Complete, "{label}");
            }
            Prior::Fresh => unreachable!(),
        }
    }

    // The current managed run: admitted, attached, its workload applied.
    let mut run = begin(&cx, &root, task(OWNER), 2, None, acknowledge, &mut fake).expect("admit");
    attach(&mut run);
    apply(&mut run, &fake, "work").expect("workload");
    if front == Front::Detached {
        requested(&mut run, &root, RequestAction::Detach, 10).expect("detach");
    }

    // The stop.
    let transition_op = match stop {
        Stop::Handover => Some("handover"),
        Stop::Down => Some("down"),
        _ => None,
    };
    match stop {
        Stop::Request => stop_by_request(&mut run, &root, 11),
        Stop::Int | Stop::Term | Stop::Hup => {
            let signal = match stop {
                Stop::Int => libc::SIGINT,
                Stop::Term => libc::SIGTERM,
                _ => libc::SIGHUP,
            };
            run.interrupt(&cleanup(&cx), cancelled(StopCause::Signal(signal)))
                .expect("signal stop");
        }
        Stop::OwningClientKill if front == Front::Detached => {
            // A detached run outlives its client; only an explicit stop ends it.
            assert!(
                matches!(
                    status(&home, id(2)),
                    Observed::Verified(RunStatus::Live(_), _)
                ),
                "{label}: client death stopped a detached run"
            );
            stop_by_request(&mut run, &root, 11);
        }
        Stop::OwningClientKill => {
            run.interrupt(&cleanup(&cx), cancelled(StopCause::OwnerLost))
                .expect("owner lost");
        }
        Stop::OwnerKill => {}
        Stop::Handover | Stop::Down => {
            // The environment transition is refused while a holder exists (R4).
            let refused = transition(&home, 20, transition_op.unwrap(), Settlement::Retain, false)
                .expect("transition attempt")
                .is_none();
            assert_eq!(
                refused,
                attachment.is_some(),
                "{label}: transition while held"
            );
            stop_by_request(&mut run, &root, 11);
        }
    }
    if stop == Stop::OwnerKill {
        drop(run);
        assert!(
            matches!(
                status(&home, id(2)),
                Observed::Verified(RunStatus::Interrupted { .. }, _)
            ),
            "{label}: owner death not interrupted"
        );
    } else {
        let outcome = settle(run, &cx, &mut fake);
        assert!(outcome.is_some(), "{label}: settle");
    }
    // Recovery is idempotent: the one settle, twice.
    for _ in 0..2 {
        let outcome = settle_interrupted(
            &cx,
            &root,
            &task(OWNER),
            id(2),
            next(),
            &cleanup(&cx),
            &mut fake,
        )
        .expect("recovery settle");
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete, "{label}");
    }
    // An operator clears the residue; its holder protection ends only now.
    if prior == Prior::ClosedWithResidue {
        std::fs::write(home.join("unstick"), b"").expect("unstick");
    }
    if prior != Prior::Fresh {
        let outcome = settle_interrupted(
            &cx,
            &root,
            &task(OWNER),
            id(1),
            next(),
            &cleanup(&cx),
            &mut fake,
        )
        .expect("prior settle");
        assert_eq!(outcome.cleanup(), CleanupOutcome::Complete, "{label}");
    }
    // The transition proceeds once nothing holds; exclusive stop-on-release
    // brings the environment down only after its release.
    if let Some(op) = transition_op {
        let fence = transition(&home, 21, op, Settlement::Retain, false).expect("transition");
        assert!(fence.is_some(), "{label}: transition after release");
    } else if matches!(
        attachment,
        Some(AttachMode::Exclusive {
            stop_on_release: true
        })
    ) {
        let fence = transition(&home, 21, "down", Settlement::Irreversible, false).expect("down");
        assert!(fence.is_some(), "{label}: stop on release");
    }

    let mut world = World::capture(&home);
    world.integrity = Some(
        crate::store::Store::open(&cx, &root)
            .expect("store")
            .check_integrity()
            .map_err(|e| format!("{e:?}")),
    );
    for oracle in [
        Oracle::O2,
        Oracle::O3,
        Oracle::O4,
        Oracle::O5,
        Oracle::O6,
        Oracle::O11,
        Oracle::O13,
    ] {
        if let Err(failure) = oracle.check(&world) {
            panic!("{label}: {failure}");
        }
    }
    // Exactly one release per attachment, whatever the cell.
    let expected = usize::from(attachment.is_some()) * if prior == Prior::Fresh { 1 } else { 2 };
    assert_eq!(world.attachments.len(), expected, "{label}: attachments");
    assert_eq!(world.releases.len(), expected, "{label}: releases");
    for (environment, run, state) in &world.attachments {
        assert_eq!(state, "released", "{label}: {environment}/{run}");
        let count = world
            .releases
            .iter()
            .filter(|(e, r)| e == environment && r == run)
            .count();
        assert_eq!(
            count, 1,
            "{label}: {environment}/{run} released {count} times"
        );
    }
    assert_eq!(
        world.calls.iter().filter(|c| *c == "release work").count(),
        1,
        "{label}: workload undo"
    );
    assert!(
        world
            .runs
            .iter()
            .all(|r| r.phase == "closed" && r.cleanup.as_deref() == Some("complete")),
        "{label}: unresolved runs {:?}",
        world.runs
    );
}

#[test]
fn matrix_supervisor_full_cross_product_releases_each_attachment_once() {
    let started = Instant::now();
    let mut cells = 0;
    for front in FRONTS {
        for prior in PRIORS {
            for stop in STOPS {
                for attachment in ATTACHMENTS {
                    cell(front, prior, stop, attachment);
                    cells += 1;
                }
            }
        }
    }
    assert_eq!(cells, 3 * 4 * 8 * 3);
    eprintln!(
        "supervisor matrix: {cells} cells in {:?}",
        started.elapsed()
    );
}
