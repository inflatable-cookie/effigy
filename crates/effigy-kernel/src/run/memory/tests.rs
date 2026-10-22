use super::*;
use crate::id::{CheckoutId, CheckoutKind, Surface, TaskId};
use crate::observe::UnknownReason;
use crate::outcome::CleanupOutcome;
use serde_json::json;

fn record(n: u8) -> ForegroundRecord {
    let owner = TaskId::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, [1; 16]),
        "",
        Surface::Published,
        "build",
        None,
    )
    .unwrap()
    .resource();
    ForegroundRecord::new(RunId::new(1, [n; 10]).unwrap(), owner)
}
fn spec(record: &ForegroundRecord) -> EffectSpec {
    EffectSpec {
        target: record.fence().clone(),
        kind: crate::custody::KIND.into(),
        payload: json!({"digest":"redacted"}),
        settlement: Settlement::Release(Value::Null),
        required: true,
    }
}

#[test]
fn foreground_receipt_is_not_natural_completion() {
    let mut run = record(1);
    let ticket = run.intend_process(spec(&run)).unwrap();
    assert_eq!(run.phase(), Phase::Starting);
    assert_eq!(run.workload(), None);
    assert!(run.advance(Phase::Active).is_err());
    run.applied(&ticket, json!({"pid":42})).unwrap();
    run.advance(Phase::Active).unwrap();
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    assert_eq!(run.phase(), Phase::Active);
    run.completed(&ticket, RequiredObligation::ProcessExit(0))
        .unwrap();
    assert_eq!(run.effects()[0].receipt.as_ref().unwrap()["exit"], 0);
    run.finish(WorkloadOutcome::Succeeded).unwrap();
    assert_eq!(run.workload(), Some(&WorkloadOutcome::Succeeded));
    assert!(run.close().is_err());
    run.settle_effect(ticket.sequence, EffectState::Released, None)
        .unwrap();
    run.close().unwrap();
    assert_eq!(run.phase(), Phase::Closed);
    assert!(run.intend_process(spec(&run)).is_err());
    assert!(run
        .settle_effect(ticket.sequence, EffectState::Residue, None)
        .is_err());
}

#[test]
fn foreground_ticket_binds_run_incarnation_even_with_equal_resource_fences() {
    let mut first = record(1);
    let mut second = record(2);
    assert_eq!(first.fence(), second.fence());
    assert_ne!(first.incarnation(), second.incarnation());
    let first_ticket = first.intend_process(spec(&first)).unwrap();
    let second_ticket = second.intend_process(spec(&second)).unwrap();
    assert!(second.applied(&first_ticket, json!({})).is_err());
    assert!(second
        .failed(&first_ticket, AbsenceProof::NoSuchProcess { pid: 42 })
        .is_err());
    assert!(second
        .unknown(&first_ticket, UnknownReason::GuardUnavailable)
        .is_err());
    assert!(second
        .completed(&first_ticket, RequiredObligation::Applied)
        .is_err());
    second.applied(&second_ticket, json!({})).unwrap();
    assert!(second.applied(&second_ticket, json!({})).is_err());
    second
        .completed(&second_ticket, RequiredObligation::ProcessExit(0))
        .unwrap();
    assert!(second
        .completed(&second_ticket, RequiredObligation::ProcessExit(0))
        .is_err());
    assert_eq!(first.effects()[0].state, EffectState::Intended);
}

#[test]
fn foreground_refuses_durable_and_foreign_effects_before_intent() {
    let mut run = record(1);
    for kind in [
        "kernel:remote",
        "kernel:attachment",
        "kernel:claim",
        "kernel:child",
        "kernel:socket",
    ] {
        let mut s = spec(&run);
        s.kind = kind.into();
        assert!(run.intend_process(s.clone()).is_err());
        assert!(run.intend(s).is_err());
    }
    for settlement in [Settlement::Retain, Settlement::Irreversible] {
        let mut s = spec(&run);
        s.kind = "engine".into();
        s.settlement = settlement;
        assert!(run.intend(s).is_err());
    }
    let mut s = spec(&run);
    s.target.generation = Generation::new(1);
    assert!(run.intend_process(s).is_err());
    assert!(run.effects().is_empty());
    assert_eq!(run.phase(), Phase::Admitted);
    assert_eq!(run.workload(), Some(&WorkloadOutcome::NotStarted));
}

#[test]
fn foreground_unknown_and_absence_are_distinct_and_retry_is_fenced() {
    let mut run = record(1);
    let ticket = run.intend_process(spec(&run)).unwrap();
    run.unknown(&ticket, UnknownReason::GuardUnavailable)
        .unwrap();
    assert_eq!(run.effects()[0].state, EffectState::Unknown);
    run.failed(&ticket, AbsenceProof::NoSuchProcess { pid: 42 })
        .unwrap();
    assert_eq!(run.effects()[0].state, EffectState::Failed);
    assert!(run.applied(&ticket, json!({})).is_err());
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    run.interrupt(WorkloadOutcome::NotStarted).unwrap();
    run.close().unwrap();
}

#[test]
fn foreground_failed_exit_cannot_manufacture_success() {
    let mut run = record(1);
    let ticket = run.intend_process(spec(&run)).unwrap();
    run.applied(&ticket, json!({})).unwrap();
    run.completed(&ticket, RequiredObligation::ProcessExit(7))
        .unwrap();
    assert!(run.finish(WorkloadOutcome::Succeeded).is_err());
    run.finish(WorkloadOutcome::Failed { code: 7 }).unwrap();
    run.settle_effect(ticket.sequence, EffectState::Released, None)
        .unwrap();
    run.close().unwrap();
    assert_eq!(run.workload(), Some(&WorkloadOutcome::Failed { code: 7 }));
}

#[test]
fn foreground_residue_is_explicit_and_never_positive_cleanup() {
    let mut run = record(1);
    let ticket = run.intend_process(spec(&run)).unwrap();
    run.applied(&ticket, json!({})).unwrap();
    assert!(run
        .settle_effect(ticket.sequence, EffectState::Applied, None)
        .is_err());
    assert!(run.close().is_err());
    run.settle_effect(
        ticket.sequence,
        EffectState::Residue,
        Some("anchor unavailable".into()),
    )
    .unwrap();
    run.close().unwrap();
    assert_eq!(run.workload(), Some(&WorkloadOutcome::Interrupted));
    assert_eq!(run.effects()[0].state, EffectState::Residue);
    assert_eq!(
        run.effects()[0].reason.as_deref(),
        Some("anchor unavailable")
    );
}

#[test]
fn foreground_record_has_no_filesystem_or_database_dependency() {
    // This is a record-level accounting proof, not the CLI write-count proof.
    // No state root or SQLite connection is supplied or constructed, and the
    // complete lifecycle works with no HOME or checkout lookup.
    for n in 1..=20 {
        let mut run = record(n);
        let ticket = run.intend_process(spec(&run)).unwrap();
        run.applied(&ticket, json!({})).unwrap();
        run.completed(&ticket, RequiredObligation::ProcessExit(0))
            .unwrap();
        run.finish(WorkloadOutcome::Succeeded).unwrap();
        run.settle_effect(ticket.sequence, EffectState::Released, None)
            .unwrap();
        run.close().unwrap();
        assert!(Outcome::new(
            Ok(()),
            run.workload().unwrap().clone(),
            CleanupOutcome::Complete,
            &[]
        )
        .is_ok());
    }
}
