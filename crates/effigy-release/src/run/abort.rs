//! `effigy release abort`: discard an unpublished failed Run (contract 054 §5, §12).
//!
//! A Run whose publish targets are all still `pending`, whose release commit
//! never left the checkout and whose journal shows no publish outcome in doubt
//! has nothing irreversible behind it. Abort removes the Run's record, receipt
//! and candidate and clears its checkpoint, so the next `release run` starts a
//! fresh Run. It never runs `git reset`: a prepared commit stays where it is
//! unless the operator resets it, and the report names the exact command.
//!
//! The gate is checked on pure reads before the journal changes, so a refusal
//! leaves no state behind. It is checked again under the owner gate before
//! anything is removed.

use std::path::Path;
use std::process::Command as ProcessCommand;

use effigy_kernel::id::RunId;
use effigy_kernel::outcome::{CleanupOutcome, WorkloadOutcome};
use effigy_kernel::run::{Effect, EffectSpec, EffectState, Run, Settlement};
use serde::Serialize;

use super::kernel::{
    new_run_id, read_release_observation, ReleaseSession, ReleaseSettlement, ReleaseState,
    ReportsOnly, COMMIT_PUSH_EFFECT, TAG_PUSH_EFFECT, TASK_PUBLISH_EFFECT,
};
use super::record::{RecordTargetState, ReleaseRecord, ReleaseRunStatus};
use crate::git::git_head_sha;
use crate::ReleaseError;

/// The abort report's schema, versioned like the other release payloads.
pub const RELEASE_ABORT_SCHEMA: &str = "effigy.release.abort.v1";

/// The journal kind of the state removal. It is retained, like the record
/// writes, because a removed directory is not an irreversible publish.
const STATE_REMOVE_EFFECT: &str = "release:state-remove";

/// The release commit, as the checkout holds it after the discard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreparedCommit {
    /// The commit `prepare` made, or `None` when the release made none.
    pub commit: Option<String>,
    /// The prepared commit is `HEAD`.
    pub on_head: bool,
    /// The prepared commit is reachable from a remote-tracking branch.
    pub pushed: bool,
}

/// What an abort discarded and what it left for the operator to decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseAbortReport {
    pub schema: &'static str,
    /// `discarded`: abort only reports a discard, since every refusal is an error.
    pub status: &'static str,
    pub release_id: String,
    pub run_id: String,
    /// The release state directory that was removed.
    pub state_directory: String,
    pub source_commit: Option<String>,
    pub head_commit: String,
    pub prepared_commit: PreparedCommit,
    /// The git command that discards an unpushed prepared commit sitting on
    /// `HEAD`. Abort never runs it.
    pub reset_hint: Option<String>,
    pub messages: Vec<String>,
}

/// Discard the unpublished release Run of this checkout.
///
/// `requested` names the Run, when given, and must be the active one. Without
/// `confirmed`, every gate still runs and nothing changes: the refusal says
/// what a confirmed abort would discard.
pub fn abort_release(
    home: &Path,
    repo_root: &Path,
    requested: Option<&str>,
    confirmed: bool,
) -> Result<ReleaseAbortReport, ReleaseError> {
    let observation = read_release_observation(home, repo_root)?.ok_or_else(|| {
        ReleaseError::TaskInvocation(format!(
            "there is no release Run for {} to abort; `effigy release status` shows the active Run",
            repo_root.display()
        ))
    })?;
    let record = &observation.record;
    check_requested(record, requested)?;
    check_settled(record, observation.settlement)?;
    verdict(record, &observation.run_effects)?;

    let head = git_head_sha(repo_root)?;
    let prepared = prepared_commit(repo_root, &head, record)?;
    if !confirmed {
        return Err(ReleaseError::TaskInvocation(format!(
            "`effigy release abort` discards release Run {} and its local release state, and clears its checkpoint; it is unpublished, so this is safe, but it needs `--yes` to proceed",
            record.release_id
        )));
    }
    discard(home, repo_root, record, head, prepared)
}

/// The text a person reads: one line per fact the discard settled.
pub fn render_release_abort_text(report: &ReleaseAbortReport) -> String {
    report.messages.join("\n")
}

/// The `effigy.release.abort.v1` payload.
pub fn render_release_abort_json(report: &ReleaseAbortReport) -> String {
    serde_json::to_string_pretty(report).expect("JSON value encodes")
}

fn check_requested(record: &ReleaseRecord, requested: Option<&str>) -> Result<(), ReleaseError> {
    let Some(run) = requested.map(str::trim) else {
        return Ok(());
    };
    if run == record.release_id || run == record.run_id {
        return Ok(());
    }
    let stage = if record.stage.is_empty() {
        "validate"
    } else {
        record.stage.as_str()
    };
    Err(ReleaseError::TaskInvocation(format!(
        "`{run}` is not this checkout's release Run; the active release Run is {} (stage `{stage}`, status `{}`)",
        record.release_id,
        record.status.label()
    )))
}

fn check_settled(
    record: &ReleaseRecord,
    settlement: ReleaseSettlement,
) -> Result<(), ReleaseError> {
    match settlement {
        ReleaseSettlement::Settled => Ok(()),
        ReleaseSettlement::Live => Err(ReleaseError::TaskInvocation(format!(
            "release Run {} is still running in another invocation; abort can discard it once that invocation finishes",
            record.release_id
        ))),
        ReleaseSettlement::Interrupted | ReleaseSettlement::Incomplete => {
            Err(ReleaseError::TaskInvocation(format!(
                "release Run {} did not settle; `effigy release settle {}` settles it, and abort can then discard it",
                record.release_id, record.release_id
            )))
        }
    }
}

/// The refusal that keeps an outcome in doubt, or a published destination,
/// from being discarded. It reads the record and the Run's journal.
fn verdict(record: &ReleaseRecord, effects: &[Effect]) -> Result<(), ReleaseError> {
    let id = &record.release_id;
    if record.status == ReleaseRunStatus::Complete {
        return Err(ReleaseError::TaskInvocation(format!(
            "release Run {id} is complete, and a completed release is never discarded; `effigy release receipt {id}` shows what it recorded"
        )));
    }
    if record.pushed_commit {
        return Err(ReleaseError::TaskInvocation(format!(
            "the release commit of Run {id} has been pushed to its branch, and abort cannot undo a push; `effigy release resume {id}` finishes the Run, and `effigy release receipt {id}` shows what it recorded"
        )));
    }
    for target in &record.targets {
        let next = match target.state {
            RecordTargetState::Pending => continue,
            RecordTargetState::Published => {
                format!("`effigy release resume {id}` continues the Run past it")
            }
            RecordTargetState::Unknown => format!(
                "`effigy release resume {id}` reconciles it, and `effigy release decide {id} --published` or `effigy release decide {id} --not-published` records its outcome when the Run waits for one"
            ),
            RecordTargetState::Failed => format!(
                "a failed publish may have landed some destinations, so abort never discards it; `effigy release resume {id}` retries it from the checkpoint"
            ),
        };
        return Err(ReleaseError::TaskInvocation(format!(
            "release target `{}` is {}, so Run {id} cannot be discarded; {next}",
            target.name,
            target.state.label()
        )));
    }
    for effect in effects {
        if !publish_in_doubt(&effect.spec.kind, effect.state) {
            continue;
        }
        return Err(ReleaseError::TaskInvocation(format!(
            "the journal records `{}` as {}, so its outcome is not settled; `effigy release receipt {id}` shows it, and `effigy release resume {id}` reconciles it",
            effect.spec.kind,
            effect.state.label()
        )));
    }
    Ok(())
}

/// Whether one journalled effect may have reached a destination. A publish
/// that the journal records as anything but `failed` is in doubt; a state
/// removal or record write never is.
fn publish_in_doubt(kind: &str, state: EffectState) -> bool {
    matches!(
        kind,
        TAG_PUSH_EFFECT | COMMIT_PUSH_EFFECT | TASK_PUBLISH_EFFECT
    ) && state != EffectState::Failed
}

/// The prepared commit, when the release made one, and whether it is `HEAD`
/// and reachable from a remote-tracking branch. Read before anything changes.
fn prepared_commit(
    repo_root: &Path,
    head: &str,
    record: &ReleaseRecord,
) -> Result<PreparedCommit, ReleaseError> {
    let commit = record
        .candidate_commit
        .clone()
        .filter(|candidate| record.source_commit.as_deref() != Some(candidate.as_str()));
    let Some(commit) = commit else {
        return Ok(PreparedCommit {
            commit: None,
            on_head: false,
            pushed: false,
        });
    };
    Ok(PreparedCommit {
        on_head: head == commit,
        pushed: remote_contains(repo_root, &commit)?,
        commit: Some(commit),
    })
}

fn remote_contains(repo_root: &Path, commit: &str) -> Result<bool, ReleaseError> {
    let output = ProcessCommand::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["branch", "--remotes", "--contains", commit])
        .output()
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "release abort could not read the remote branches: {error}; nothing was discarded"
            ))
        })?;
    if !output.status.success() {
        return Err(ReleaseError::TaskInvocation(
            "release abort could not read the remote branches; nothing was discarded".to_owned(),
        ));
    }
    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn reset_hint(prepared: &PreparedCommit, source: Option<&str>) -> Option<String> {
    match (prepared.on_head, prepared.pushed, source) {
        (true, false, Some(source)) => Some(format!("git reset --keep {source}")),
        _ => None,
    }
}

fn discard(
    home: &Path,
    repo_root: &Path,
    record: &ReleaseRecord,
    head: String,
    prepared: PreparedCommit,
) -> Result<ReleaseAbortReport, ReleaseError> {
    let session = ReleaseSession::open(home, repo_root)?;
    let state = session.state_for_owner();
    let directory = state.directory();
    let id = session
        .known_run(&record.run_id)?
        .ok_or_else(|| ReleaseError::TaskInvocation("missing release invocation".into()))?;
    let mut adapter = ReportsOnly;
    let mut run = session.begin(new_run_id()?, &mut adapter)?;

    let result = discard_under_gate(&session, &state, &mut run, record, id);
    let workload = match &result {
        Ok(()) => WorkloadOutcome::Succeeded,
        Err(_) => WorkloadOutcome::Failed { code: 1 },
    };
    run.finish(workload).map_err(kernel_error)?;
    let settled = run
        .settle(&session.cleanup()?, &mut adapter)
        .map_err(kernel_error)?;
    if settled.cleanup() != CleanupOutcome::Complete {
        return Err(ReleaseError::TaskInvocation(
            "release abort settlement is incomplete; `effigy release status` shows the pending cleanup"
                .to_owned(),
        ));
    }
    result?;

    let source = record.source_commit.clone();
    let hint = reset_hint(&prepared, source.as_deref());
    let messages = messages(record, &directory, &prepared, hint.as_deref());
    Ok(ReleaseAbortReport {
        schema: RELEASE_ABORT_SCHEMA,
        status: "discarded",
        release_id: record.release_id.clone(),
        run_id: record.run_id.clone(),
        state_directory: directory.display().to_string(),
        source_commit: source,
        head_commit: head,
        prepared_commit: prepared,
        reset_hint: hint,
        messages,
    })
}

/// The gate again, under the owner gate: the same checkpoint and the same
/// record, and no publish outcome in doubt. Then the removal.
fn discard_under_gate(
    session: &ReleaseSession,
    state: &ReleaseState,
    run: &mut Run<'_>,
    record: &ReleaseRecord,
    id: RunId,
) -> Result<(), ReleaseError> {
    let changed = || {
        ReleaseError::TaskInvocation(format!(
            "release Run {} changed while abort waited for the release gate; nothing was discarded, and `effigy release status` shows it",
            record.release_id
        ))
    };
    let checkpoint = session.read_checkpoint()?;
    if checkpoint
        .as_ref()
        .map(|checkpoint| checkpoint.run_id.as_str())
        != Some(record.run_id.as_str())
    {
        return Err(changed());
    }
    if state.read_record()?.as_ref() != Some(record) {
        return Err(changed());
    }
    verdict(record, &session.journal_effects(id)?)?;

    // The directory goes before the checkpoint clears. A crash between the two
    // leaves a checkpoint with no record, which refuses to resume. The other
    // order would leave a record with no checkpoint, which `release run`
    // would silently continue.
    let directory = state.directory();
    let ticket = run
        .intend(EffectSpec {
            target: run.fence().clone(),
            kind: STATE_REMOVE_EFFECT.into(),
            payload: serde_json::json!({ "directory": directory.display().to_string() }),
            settlement: Settlement::Retain,
            required: false,
        })
        .map_err(kernel_error)?;
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to remove the release state at {}: {error}",
                directory.display()
            ))
        })?;
    }
    run.applied(&ticket, serde_json::Value::Null)
        .map_err(kernel_error)?;
    run.clear_engine_record().map_err(kernel_error)
}

fn messages(
    record: &ReleaseRecord,
    directory: &Path,
    prepared: &PreparedCommit,
    reset_hint: Option<&str>,
) -> Vec<String> {
    let mut lines = vec![format!(
        "discarded release Run {}: removed {} and cleared its checkpoint",
        record.release_id,
        directory.display()
    )];
    match (&prepared.commit, prepared.on_head, prepared.pushed) {
        (None, _, _) => {}
        (Some(commit), true, false) => {
            let short = &commit[..commit.len().min(12)];
            let command = reset_hint.unwrap_or("git reset --keep <source>");
            lines.push(format!(
                "the prepared commit {short} still sits on HEAD and is unpushed; abort left it in place. To discard it too, run `{command}` before the next `effigy release run`"
            ));
        }
        (Some(commit), _, _) if !prepared.on_head => {
            let short = &commit[..commit.len().min(12)];
            lines.push(format!(
                "the prepared commit {short} is no longer on HEAD, so there is nothing to reset"
            ));
        }
        (Some(_), _, _) => {
            lines.push(
                "the prepared commit is on HEAD and reachable from a remote branch; abort does not reset or push it"
                    .to_owned(),
            );
        }
    }
    lines
}

fn kernel_error(error: impl std::fmt::Display) -> ReleaseError {
    ReleaseError::TaskInvocation(format!("release abort: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run::record::RecordTarget;

    fn record_with(status: ReleaseRunStatus) -> ReleaseRecord {
        let mut record = ReleaseRecord::new("run-1", Path::new("/repo"), "2026-11-28T00:00:00Z");
        record.status = status;
        record
    }

    fn target(name: &str, state: RecordTargetState) -> RecordTarget {
        RecordTarget {
            name: name.to_owned(),
            kind: "tag".to_owned(),
            state,
            destinations: Vec::new(),
            note: None,
        }
    }

    #[test]
    fn a_run_whose_targets_are_all_pending_is_discardable() {
        let mut record = record_with(ReleaseRunStatus::Failed);
        record.targets = vec![target("tags", RecordTargetState::Pending)];
        assert!(verdict(&record, &[]).is_ok());
    }

    #[test]
    fn a_complete_run_is_never_discarded() {
        let record = record_with(ReleaseRunStatus::Complete);
        let message = verdict(&record, &[]).unwrap_err().to_string();
        assert!(message.contains("is complete"), "{message}");
    }

    #[test]
    fn a_pushed_release_commit_is_never_discarded() {
        let mut record = record_with(ReleaseRunStatus::Failed);
        record.pushed_commit = true;
        let message = verdict(&record, &[]).unwrap_err().to_string();
        assert!(message.contains("pushed to its branch"), "{message}");
    }

    #[test]
    fn every_target_that_left_pending_refuses_with_its_next_step() {
        for (state, next) in [
            (RecordTargetState::Published, "continues the Run"),
            (RecordTargetState::Unknown, "reconciles it"),
            (RecordTargetState::Failed, "retries it"),
        ] {
            let mut record = record_with(ReleaseRunStatus::Failed);
            record.targets = vec![target("tags", state)];
            let message = verdict(&record, &[]).unwrap_err().to_string();
            assert!(message.contains("release target `tags`"), "{message}");
            assert!(message.contains(next), "{state:?}: {message}");
        }
    }

    #[test]
    fn only_an_unsettled_publish_in_the_journal_blocks_the_discard() {
        assert!(publish_in_doubt(COMMIT_PUSH_EFFECT, EffectState::Unknown));
        assert!(publish_in_doubt(TAG_PUSH_EFFECT, EffectState::Intended));
        assert!(publish_in_doubt(TASK_PUBLISH_EFFECT, EffectState::Applied));
        assert!(!publish_in_doubt(TAG_PUSH_EFFECT, EffectState::Failed));
        assert!(!publish_in_doubt(
            "release:checkpoint-file",
            EffectState::Unknown
        ));
    }

    #[test]
    fn a_requested_run_must_be_the_active_one() {
        let record = record_with(ReleaseRunStatus::Failed);
        assert!(check_requested(&record, None).is_ok());
        assert!(check_requested(&record, Some(record.release_id.as_str())).is_ok());
        let message = check_requested(&record, Some("other"))
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("not this checkout's release Run"),
            "{message}"
        );
    }

    #[test]
    fn the_reset_hint_names_the_source_only_for_an_unpushed_prepared_commit_on_head() {
        let prepared = |on_head, pushed| PreparedCommit {
            commit: Some("abc".to_owned()),
            on_head,
            pushed,
        };
        assert_eq!(
            reset_hint(&prepared(true, false), Some("src")),
            Some("git reset --keep src".to_owned())
        );
        assert_eq!(reset_hint(&prepared(false, false), Some("src")), None);
        assert_eq!(reset_hint(&prepared(true, true), Some("src")), None);
        assert_eq!(reset_hint(&prepared(true, false), None), None);
    }
}
