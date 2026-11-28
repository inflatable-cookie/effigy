//! `effigy release abort` through the real release Run (contract 054 §5, §12).
//!
//! An unpublished failed Run is discarded, and the next `release run` starts a
//! fresh one. A Run with a publish outcome in doubt, or a published
//! destination, is refused, and the refusal changes no state.

use super::*;
use crate::run::{abort_release, render_release_abort_json, ReleaseAbortReport};

/// The directory a Run's record and receipt live in.
fn state_dir(outcome: &ReleaseRunOutcome) -> PathBuf {
    outcome
        .receipt_path
        .parent()
        .expect("the receipt sits in the release state directory")
        .to_path_buf()
}

/// The record, the receipt and the journal, as bytes. A refusal leaves every
/// one of them unchanged.
fn release_state_bytes(fixture: &Fixture, state_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for name in ["record.json", "receipt.json"] {
        if let Ok(bytes) = std::fs::read(state_dir.join(name)) {
            files.push((name.to_owned(), bytes));
        }
    }
    for name in ["kernel.db", "kernel.db-wal"] {
        if let Ok(bytes) = std::fs::read(fixture.root.join(".effigy/state").join(name)) {
            files.push((name.to_owned(), bytes));
        }
    }
    files
}

/// The abort is refused with `needle` in its message, and nothing changes.
fn refused_without_change(
    fixture: &Fixture,
    state_dir: &Path,
    requested: Option<&str>,
    confirmed: bool,
    needle: &str,
) {
    let before = release_state_bytes(fixture, state_dir);
    let record_before = current_record(fixture);
    let error = abort_release(&fixture.home, &fixture.root, requested, confirmed)
        .expect_err("the abort is refused");
    let message = error.to_string();
    assert!(message.contains(needle), "{message}");
    assert_eq!(
        release_state_bytes(fixture, state_dir),
        before,
        "a refused abort changes no state: {message}"
    );
    assert_eq!(current_record(fixture), record_before);
}

/// A project whose certify gate fails after the release commit is prepared:
/// the Run stops failed, with a prepared commit and nothing published.
fn failed_certify_run(fixture: &Fixture) -> (TestEffects, ReleaseRunOutcome) {
    single_identity_project(fixture);
    write(&fixture.root, "gates/certify.sh", "#!/bin/sh\nexit 3\n");
    commit_and_push(fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    let failed = run(fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(failed.record.status, ReleaseRunStatus::Failed);
    (effects, failed)
}

#[test]
fn an_unpublished_failed_run_aborts_and_the_next_run_starts_fresh() {
    let fixture = fixture("abort-fresh");
    let (_, failed) = failed_certify_run(&fixture);
    let source = failed
        .record
        .source_commit
        .clone()
        .expect("a source commit");
    let candidate = failed
        .record
        .candidate_commit
        .clone()
        .expect("the prepared commit");
    assert_ne!(candidate, source, "prepare made a commit");
    assert_eq!(git(&fixture.root, &["rev-parse", "HEAD"]), candidate);
    let state = state_dir(&failed);
    assert!(state.is_dir());

    let report: ReleaseAbortReport =
        abort_release(&fixture.home, &fixture.root, None, true).expect("the abort discards");
    assert_eq!(report.status, "discarded");
    assert_eq!(report.release_id, failed.record.release_id);
    assert!(!state.exists(), "the release state directory is removed");
    assert!(
        read_release_observation(&fixture.home, &fixture.root)
            .expect("a pure read")
            .is_none(),
        "the checkpoint is cleared, so no Run is active"
    );

    // The prepared commit stays on HEAD: abort never resets. The report says
    // exactly how to discard it.
    assert_eq!(
        report.prepared_commit.commit.as_deref(),
        Some(candidate.as_str())
    );
    assert!(report.prepared_commit.on_head);
    assert!(!report.prepared_commit.pushed);
    assert_eq!(
        report.reset_hint.as_deref(),
        Some(format!("git reset --keep {source}").as_str())
    );
    assert_eq!(git(&fixture.root, &["rev-parse", "HEAD"]), candidate);

    // The JSON payload carries the same facts under its schema.
    let json: serde_json::Value =
        serde_json::from_str(&render_release_abort_json(&report)).expect("JSON");
    assert_eq!(json["schema"], "effigy.release.abort.v1");
    assert_eq!(json["status"], "discarded");
    assert_eq!(json["prepared_commit"]["on_head"], true);
    assert_eq!(json["prepared_commit"]["pushed"], false);

    // Discard the prepared commit as the report says, fix the gate, and a
    // fresh `release run` starts a new Run instead of resuming the old one.
    git(&fixture.root, &["reset", "--keep", source.as_str()]);
    write(
        &fixture.root,
        "gates/certify.sh",
        "#!/bin/sh\necho certified\n",
    );
    commit_all(&fixture.root, "fix the certify gate");

    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    let fresh = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert!(fresh.succeeded(), "fresh run: {:?}", fresh.record.errors);
    assert_eq!(fresh.record.status, ReleaseRunStatus::Complete);
    assert_ne!(
        fresh.record.release_id, failed.record.release_id,
        "the next run is a new Run, not the aborted one"
    );
    assert!(remote_tags(&fixture).contains_key("v0.2.5"));
}

#[test]
fn an_abort_refuses_a_run_whose_tag_was_published_and_changes_nothing() {
    let fixture = fixture("abort-published");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    // The tag lands, then the closeout push fails: a published destination.
    effects.fail_commit_push = true;
    let failed = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(
        failed.record.target("tags").expect("tag target").state,
        RecordTargetState::Published
    );
    assert!(!failed.record.pushed_commit);
    assert!(remote_tags(&fixture).contains_key("v0.2.5"));

    refused_without_change(
        &fixture,
        &state_dir(&failed),
        None,
        true,
        "release target `tags` is published",
    );
}

#[test]
fn an_abort_refuses_a_run_whose_tag_push_outcome_is_unknown_and_changes_nothing() {
    let fixture = fixture("abort-unknown");
    single_identity_project(&fixture);
    let mut effects = TestEffects::new(&fixture.root);
    effects.artifact_path = Some("dist/app-0.2.5.tgz".to_owned());
    // The push really happens, and its outcome is not recorded.
    effects.ambiguous_push_on = Some(1);
    let failed = run(&fixture, &mut effects, ReleaseAction::Run, None);
    assert_eq!(
        failed.record.target("tags").expect("tag target").state,
        RecordTargetState::Unknown
    );

    refused_without_change(
        &fixture,
        &state_dir(&failed),
        None,
        true,
        "release target `tags` is unknown",
    );
    // The tag is still there: the refusal did not touch the remote either.
    assert!(remote_tags(&fixture).contains_key("v0.2.5"));
}

#[test]
fn an_abort_without_yes_checks_every_gate_and_changes_nothing() {
    let fixture = fixture("abort-confirm");
    let (_, failed) = failed_certify_run(&fixture);
    let state = state_dir(&failed);

    refused_without_change(&fixture, &state, None, false, "--yes");
    assert!(state.is_dir(), "the state survives an unconfirmed abort");

    abort_release(&fixture.home, &fixture.root, None, true).expect("the confirmed abort");
    assert!(!state.exists());
}

#[test]
fn an_abort_of_a_run_that_is_not_active_is_refused() {
    let fixture = fixture("abort-named");
    let (_, failed) = failed_certify_run(&fixture);
    let state = state_dir(&failed);

    refused_without_change(
        &fixture,
        &state,
        Some("not-a-run"),
        true,
        "not this checkout's release Run",
    );
    // The active Run, named by its own id, is the one that aborts.
    abort_release(
        &fixture.home,
        &fixture.root,
        Some(failed.record.release_id.as_str()),
        true,
    )
    .expect("the named active Run aborts");
    assert!(!state.exists());
}

#[test]
fn an_abort_with_no_release_run_says_so_and_creates_nothing() {
    let fixture = fixture("abort-none");
    single_identity_project(&fixture);

    let error = abort_release(&fixture.home, &fixture.root, None, true)
        .expect_err("a checkout with no release Run has nothing to abort");
    assert!(
        error.to_string().contains("there is no release Run"),
        "{error}"
    );
    assert!(!fixture.root.join(".effigy/state/release").exists());
}
