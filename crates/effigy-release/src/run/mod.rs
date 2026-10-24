//! The checkpointed release Run (contract 054 §3, §5 and §7).
//!
//! `validate → [pre-gates] → prepare → certify → publish → verify → closeout`
//! runs as one durable kernel Run per invocation, with a checkpoint written to
//! Effigy-owned state after every stage boundary. A resume continues from that
//! checkpoint with the same candidate commit, versions and artifact digests;
//! it never re-derives or rebuilds them. Publish is irreversible and every tag
//! push is journalled before it runs; a resume reconciles an unknown push by
//! looking the tag up first (054 §7).

mod artifacts;
mod effects;
mod input;
mod kernel;
mod receipt;
mod record;

#[cfg(test)]
mod tests;

pub use artifacts::{capture_artifact, sha256_bytes, verify_artifact_digests};
pub use effects::{
    approver_name, GitEffects, HostedEvidence, ReleaseEffects, TaskRequest, TaskResponse,
};
pub use input::{
    parse_release_task_output, read_release_task_output, release_input_value, Committed,
    ReleaseTaskOutput, ReleaseTaskSubject, RELEASE_INPUT_ENV, RELEASE_OUTPUT_ENV,
    RELEASE_TASK_INPUT_SCHEMA, RELEASE_TASK_OUTPUT_SCHEMA,
};
pub use kernel::{
    new_run_id, release_home, release_owner, ReleaseSession, ReleaseState, ReportsOnly,
};
pub use kernel::{read_release_record, read_release_record_for_root};
pub use receipt::{
    render_release_receipt_json, render_release_receipt_text, render_release_run_json,
    ReleaseReceipt,
};
pub use record::{
    RecordApproval, RecordArtifact, RecordArtifactFile, RecordCheckpoint, RecordDecision,
    RecordDestination, RecordGate, RecordIdentity, RecordTarget, RecordTargetState,
    RecordVerification, ReleaseCheckpoint, ReleaseRecord, ReleaseRunStatus, ReleaseStage,
    TargetDecision, RELEASE_RECEIPT_SCHEMA, RELEASE_RECORD_SCHEMA,
};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use effigy_changelog as changelog;
use effigy_kernel::observe::UnknownReason;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::{EffectSpec, EffectTicket, Run, Settlement};

use crate::hosted_evidence::{HostedEvidenceBinding, HostedEvidenceSpec};
use crate::model::{FileMutationApply, FileMutationPlan};
use crate::pipeline::{
    plan_release_pipeline, GateStage, IdentityKind, ReleaseIdentityPlan, ReleasePipelinePlan,
    TargetKind,
};
use crate::prepare_helpers::build_sync_mutations;
use crate::version::render_updated_version_contents;
use crate::ResolvedGate;
use crate::{
    render_prepared_changelog_contents, run_release_gates_with_observer, GateObserver,
    ReleaseError, ResolvedSyncFile, SilentGateObserver,
};

const ANNOTATED_TAG_MESSAGE: &str = "Release {tag}";
const RELEASE_RECEIPT_FILE: &str = "receipt.json";

/// What a release invocation asks for (contract 054 §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseAction {
    /// Start a Run, or continue the active one.
    Run,
    /// Continue an interrupted Run from its last checkpoint.
    Resume { run: String },
    /// Pass an approval checkpoint, then continue.
    Approve { run: String },
    /// Record an operator's ruling on an unknown publish outcome, then
    /// continue (contract 054 §7).
    Decide {
        run: String,
        decision: TargetDecision,
    },
}

#[derive(Debug, Clone)]
pub struct ReleaseRunRequest {
    pub repo_root: PathBuf,
    /// Machine home for explicit migration checks; release state is checkout-local.
    pub home: PathBuf,
    pub action: ReleaseAction,
    /// The stage boundary to stop after. Defaults to `closeout`.
    pub to: Option<ReleaseStage>,
}

/// The result of one invocation: the durable record after it, and its receipt.
#[derive(Debug, Clone)]
pub struct ReleaseRunOutcome {
    pub record: ReleaseRecord,
    pub receipt_path: PathBuf,
    pub messages: Vec<String>,
}

impl ReleaseRunOutcome {
    /// The invocation did everything it was asked. A Run stopped at a
    /// checkpoint — for approval, hosted evidence or an operator decision —
    /// also counts: it is waiting, not failing.
    pub fn succeeded(&self) -> bool {
        matches!(
            self.record.status,
            ReleaseRunStatus::Complete
                | ReleaseRunStatus::Paused
                | ReleaseRunStatus::AwaitingApproval
                | ReleaseRunStatus::AwaitingHostedEvidence
                | ReleaseRunStatus::AwaitingDecision
        )
    }

    pub fn receipt(&self) -> ReleaseReceipt {
        ReleaseReceipt::from_record(&self.record)
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Run, resume or approve a release.
pub fn run_release(
    request: &ReleaseRunRequest,
    effects: &mut dyn ReleaseEffects,
) -> Result<ReleaseRunOutcome, ReleaseError> {
    run_release_with_observer(request, effects, &mut SilentGateObserver)
}

/// Run, resume or approve a release, reporting each gate and stage to
/// `observer` as it happens. A gate can take minutes, so a command plane draws
/// it live instead of waiting for the final receipt.
pub fn run_release_with_observer(
    request: &ReleaseRunRequest,
    effects: &mut dyn ReleaseEffects,
    observer: &mut dyn GateObserver,
) -> Result<ReleaseRunOutcome, ReleaseError> {
    let session = ReleaseSession::open(&request.home, &request.repo_root)?;
    let state = session.state()?;
    let receipt_path = state.record_path.with_file_name(RELEASE_RECEIPT_FILE);
    let run_id = new_run_id()?;
    let mut adapter = ReportsOnly;
    let mut run = session.begin(run_id, &mut adapter)?;
    // Read the resumable state and resolve versions under the same owner gate
    // that will perform the work. A waiter never acts on a predecessor's
    // checkpoint or clears its candidate before admission.
    let prepared = (|| {
        let plan = plan_release_pipeline(&request.repo_root)?;
        let checkpoint = session.read_checkpoint()?;
        let mut existing = state.read_record()?;
        if let Some(record) = &mut existing {
            session.project_record(record)?;
        }
        let continuation = continue_record(
            request,
            existing,
            checkpoint.as_ref(),
            &state,
            effects,
            &mut run,
        )?;
        Ok::<_, ReleaseError>((plan, continuation))
    })();
    let (plan, continuation) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            let finished = run.finish(WorkloadOutcome::Failed { code: 1 });
            let settled = run.settle(session.cleanup(), &mut adapter);
            if let Err(cleanup) = finished.and(settled.map(|_| ())) {
                return Err(ReleaseError::TaskInvocation(format!(
                    "{error}; release preparation cleanup: {cleanup}"
                )));
            }
            return Err(error);
        }
    };
    let (mut record, start) = match continuation {
        Continuation::Nothing(record) => {
            run.finish(WorkloadOutcome::Succeeded)
                .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
            run.settle(session.cleanup(), &mut adapter)
                .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
            return Ok(ReleaseRunOutcome {
                messages: vec![format!(
                    "release Run {} is already complete; nothing to resume",
                    record.release_id
                )],
                record,
                receipt_path,
            });
        }
        Continuation::RunFrom(record, stage) => (record, stage),
    };
    let run_id_text = run_id.to_string();
    record.run_id = run_id_text.clone();
    record.release_id = if record.stage.is_empty() {
        run_id_text.clone()
    } else {
        record.release_id.clone()
    };
    record.updated_at = now();

    let to = request.to.unwrap_or(ReleaseStage::Closeout);
    let mut runner = Runner {
        plan: &plan,
        record: &mut record,
        effects,
        run: &mut run,
        state: &state,
        candidate_dir: &state.candidate_dir,
        run_id: run_id_text,
        messages: Vec::new(),
        prepared: Vec::new(),
        waiting: None,
        observer,
    };
    let staged = runner.run_stages(start, to);

    let workload = match &staged {
        Ok(()) => WorkloadOutcome::Succeeded,
        Err(_) => WorkloadOutcome::Failed { code: 1 },
    };
    let mut record = runner.record.clone();
    let messages = runner.messages.clone();
    drop(runner);

    run.finish(workload).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "the kernel could not record the release Run's outcome: {error}"
        ))
    })?;
    let settled = run
        .settle(session.cleanup(), &mut adapter)
        .map_err(|error| {
            ReleaseError::TaskInvocation(format!(
            "the release Run could not be settled: {error}; `effigy release resume {}` settles it",
            record.release_id
        ))
        })?;
    if settled.cleanup() != effigy_kernel::outcome::CleanupOutcome::Complete {
        record.errors.push(format!(
            "release Run {} left unresolved cleanup; `effigy release receipt {}` lists it",
            record.run_id, record.release_id
        ));
        record.status = ReleaseRunStatus::Failed;
    }

    Ok(ReleaseRunOutcome {
        record,
        receipt_path,
        messages,
    })
}

enum Continuation {
    RunFrom(ReleaseRecord, ReleaseStage),
    Nothing(ReleaseRecord),
}

/// Decide the record to continue and the first stage to execute.
fn continue_record(
    request: &ReleaseRunRequest,
    existing: Option<ReleaseRecord>,
    checkpoint: Option<&ReleaseCheckpoint>,
    state: &ReleaseState,
    effects: &mut dyn ReleaseEffects,
    run: &mut Run<'_>,
) -> Result<Continuation, ReleaseError> {
    let requested = match &request.action {
        ReleaseAction::Run => None,
        ReleaseAction::Resume { run }
        | ReleaseAction::Approve { run }
        | ReleaseAction::Decide { run, .. } => Some(run.trim().to_owned()),
    };
    let existing = match (existing, checkpoint) {
        (Some(record), _) => Some(record),
        (None, Some(checkpoint)) => {
            if !kernel::record_path_is_owned(state, Path::new(&checkpoint.record)) {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} points its state outside this release's state directory; refusing to read {}",
                    checkpoint.release_id,
                    checkpoint.record
                )));
            }
            return Err(ReleaseError::TaskInvocation(format!(
                "release Run {} has a checkpoint but its record is missing; the release state at {} was removed or damaged, so the Run cannot be resumed without re-deriving its candidate",
                checkpoint.release_id,
                state.record_path.display()
            )));
        }
        (None, None) => None,
    };

    let Some(mut record) = existing else {
        if let Some(run) = requested {
            return Err(ReleaseError::TaskInvocation(format!(
                "there is no release Run {run} for {}; `effigy release run` starts one",
                request.repo_root.display()
            )));
        }
        return Ok(Continuation::RunFrom(
            ReleaseRecord::new("", &request.repo_root, &now()),
            ReleaseStage::Validate,
        ));
    };

    if let Some(run) = &requested {
        if run != &record.release_id && run != &record.run_id {
            return Err(ReleaseError::TaskInvocation(format!(
                "`{run}` is not this checkout's release Run; the active release Run is {} (stage `{}`, status `{}`)",
                record.release_id,
                if record.stage.is_empty() {
                    ReleaseStage::Validate.label()
                } else {
                    &record.stage
                },
                record.status.label()
            )));
        }
    }

    match request.action {
        ReleaseAction::Run => {
            if record.status == ReleaseRunStatus::Complete {
                return Ok(Continuation::RunFrom(
                    ReleaseRecord::new("", &request.repo_root, &now()),
                    ReleaseStage::Validate,
                ));
            }
        }
        ReleaseAction::Resume { .. } => {
            if record.status == ReleaseRunStatus::Complete {
                return Ok(Continuation::Nothing(record));
            }
        }
        ReleaseAction::Approve { .. } => {
            if record.status != ReleaseRunStatus::AwaitingApproval {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} is not awaiting approval (status `{}`); `effigy release approve` only continues a Run stopped at an approval checkpoint",
                    record.release_id,
                    record.status.label()
                )));
            }
            let Some(stage) = record.waiting_on.as_deref().and_then(ReleaseStage::parse) else {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} is awaiting approval but records no stage boundary",
                    record.release_id
                )));
            };
            record.approvals.push(RecordApproval {
                stage: stage.label().to_owned(),
                approved_by: approver_name(effects, run),
                approved_at: now(),
            });
            record.status = ReleaseRunStatus::Paused;
            record.waiting_on = None;
            return Ok(Continuation::RunFrom(record, stage));
        }
        ReleaseAction::Decide { decision, .. } => {
            if record.status != ReleaseRunStatus::AwaitingDecision {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} is not awaiting a decision (status `{}`); `effigy release decide` only continues a Run whose publish outcome is unknown and whose target declares no `check` task (contract 054 §7)",
                    record.release_id,
                    record.status.label()
                )));
            }
            let Some(target_name) = record.waiting_on.clone() else {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} is awaiting a decision but records no target",
                    record.release_id
                )));
            };
            let Some(target) = record
                .targets
                .iter_mut()
                .find(|target| target.name == target_name)
            else {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release Run {} is awaiting a decision about `{target_name}`, which is not one of its targets",
                    record.release_id
                )));
            };
            if target.state != RecordTargetState::Unknown {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release target `{target_name}` records outcome `{:?}` rather than unknown; there is no unknown publish for an operator to decide",
                    target.state
                )));
            }
            target.state = match decision {
                TargetDecision::Published => RecordTargetState::Published,
                TargetDecision::NotPublished => RecordTargetState::Pending,
            };
            target.note = Some(format!("operator recorded {}", decision.label()));
            record.decisions.push(RecordDecision {
                target: target_name.clone(),
                decision: decision.label().to_owned(),
                decided_by: approver_name(effects, run),
                decided_at: now(),
            });
            record.status = ReleaseRunStatus::Paused;
            record.waiting_on = None;
            return Ok(Continuation::RunFrom(record, ReleaseStage::Publish));
        }
    }

    let start = match record.stage() {
        // A failed closeout resumes at closeout: publish already happened.
        Some(ReleaseStage::Closeout) if record.status == ReleaseRunStatus::Failed => {
            ReleaseStage::Closeout
        }
        Some(stage) => stage.next().unwrap_or(ReleaseStage::Closeout),
        None => ReleaseStage::Validate,
    };
    Ok(Continuation::RunFrom(record, start))
}

fn clear_directory(directory: &Path) -> Result<(), ReleaseError> {
    if directory.exists() {
        std::fs::remove_dir_all(directory).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to clear the previous release candidate at {}: {error}",
                directory.display()
            ))
        })?;
    }
    std::fs::create_dir_all(directory).map_err(|error| {
        ReleaseError::TaskInvocation(format!(
            "failed to create the release candidate directory {}: {error}",
            directory.display()
        ))
    })
}

struct Runner<'a, 's> {
    plan: &'a ReleasePipelinePlan,
    record: &'a mut ReleaseRecord,
    effects: &'a mut dyn ReleaseEffects,
    run: &'a mut Run<'s>,
    state: &'a ReleaseState,
    candidate_dir: &'a Path,
    run_id: String,
    messages: Vec<String>,
    /// Paths a failed prepare must restore, with their pre-prepare bytes.
    prepared: Vec<(PathBuf, Option<Vec<u8>>)>,
    /// A stage that stopped at a wait instead of finishing: the Run records the
    /// status and what it waits on, then holds nothing open (054 §3, §7).
    waiting: Option<Waiting>,
    /// How each stage and gate says what it is doing, as it happens.
    observer: &'a mut dyn GateObserver,
}

struct Waiting {
    status: ReleaseRunStatus,
    subject: Option<String>,
    message: String,
}

impl<'a, 's> Runner<'a, 's> {
    /// Execute from `start` through `to`, one stage boundary at a time.
    fn run_stages(&mut self, start: ReleaseStage, to: ReleaseStage) -> Result<(), ReleaseError> {
        let mut stage = start;
        loop {
            self.observer.stage(stage.label());
            if self.awaits_approval(stage) {
                self.record.status = ReleaseRunStatus::AwaitingApproval;
                self.record.waiting_on = Some(stage.label().to_owned());
                self.persist()?;
                self.messages.push(format!(
                    "release Run {} is awaiting approval at the `{}` boundary; `effigy release approve {}` continues it",
                    self.record.release_id,
                    stage.label(),
                    self.record.release_id
                ));
                return Ok(());
            }

            self.messages.push(format!(
                "release Run {}: `{}` started",
                self.record.release_id,
                stage.label()
            ));
            if let Err(error) = self.run_stage(stage) {
                self.record.errors.push(error.to_string());
                self.record.status = ReleaseRunStatus::Failed;
                self.record.waiting_on = None;
                self.persist()?;
                return Err(error);
            }

            if let Some(waiting) = self.waiting.take() {
                self.record.status = waiting.status;
                self.record.waiting_on = waiting.subject;
                self.persist()?;
                self.messages.push(waiting.message);
                return Ok(());
            }

            self.record.stage = stage.label().to_owned();
            self.record.status = ReleaseRunStatus::Paused;
            self.record.waiting_on = None;
            self.record.checkpoints.push(RecordCheckpoint {
                stage: stage.label().to_owned(),
                run: self.run_id.clone(),
                status: self.record.status.label().to_owned(),
                at: now(),
            });
            self.messages.push(format!(
                "release Run {}: `{}` complete",
                self.record.release_id,
                stage.label()
            ));

            if stage == to {
                if stage == ReleaseStage::Closeout {
                    self.record.status = ReleaseRunStatus::Complete;
                } else {
                    self.record.status = ReleaseRunStatus::Paused;
                    self.record.waiting_on = None;
                    self.messages.push(format!(
                        "release Run {} stopped after `{}`; `effigy release resume {}` continues with `{}`",
                        self.record.release_id,
                        stage.label(),
                        self.record.release_id,
                        stage.next().map(ReleaseStage::label).unwrap_or("nothing")
                    ));
                }
                self.persist()?;
                return Ok(());
            }
            let Some(next) = stage.next() else {
                self.record.status = ReleaseRunStatus::Complete;
                self.persist()?;
                return Ok(());
            };
            stage = next;
        }
    }

    fn awaits_approval(&self, stage: ReleaseStage) -> bool {
        self.plan
            .approvals
            .iter()
            .any(|checkpoint| checkpoint == stage.label())
            && !self.record.approved(stage)
    }

    /// Stop the current stage at a wait. The runner records it after the stage
    /// returns, so a wait never records the stage as complete.
    fn wait_for(&mut self, status: ReleaseRunStatus, subject: Option<String>, message: String) {
        self.waiting = Some(Waiting {
            status,
            subject,
            message,
        });
    }

    /// Write the record and its kernel checkpoint pointer.
    fn persist(&mut self) -> Result<(), ReleaseError> {
        self.record.updated_at = now();
        self.state.write_record(self.record, self.run)?;
        let checkpoint = self.record.checkpoint(&self.state.record_path);
        let value = serde_json::to_value(&checkpoint).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to encode the release checkpoint: {error}"
            ))
        })?;
        self.run.set_engine_record(&value).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to record the release checkpoint in the kernel journal: {error}"
            ))
        })?;
        Ok(())
    }

    fn run_stage(&mut self, stage: ReleaseStage) -> Result<(), ReleaseError> {
        match stage {
            ReleaseStage::Validate => self.validate(),
            ReleaseStage::PreGates => self.run_gates(GateStage::Pre),
            ReleaseStage::Prepare => self.prepare(),
            ReleaseStage::Certify => self.certify(),
            ReleaseStage::Publish => self.publish(),
            ReleaseStage::Verify => self.verify(),
            ReleaseStage::Closeout => self.closeout(),
        }
    }

    fn validate(&mut self) -> Result<(), ReleaseError> {
        if self.record.stage.is_empty() {
            let ticket = self
                .run
                .intend(EffectSpec {
                    target: self.run.fence().clone(),
                    kind: "release:candidate-reset".into(),
                    payload: serde_json::json!({"directory":self.candidate_dir}),
                    settlement: Settlement::Irreversible,
                    required: false,
                })
                .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
            clear_directory(self.candidate_dir)?;
            self.run
                .applied(&ticket, serde_json::Value::Null)
                .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
        }
        if !self.plan.ready {
            return Err(ReleaseError::TaskInvocation(format!(
                "release validate refused the release: {}",
                self.plan.blockers.join("; ")
            )));
        }
        if self.plan.working_tree_clean != Some(true) {
            return Err(ReleaseError::TaskInvocation(format!(
                "release validate needs a clean working tree at {}; commit or discard the pending changes first",
                self.plan.repo_root.display()
            )));
        }

        self.record.branch = Some(self.effects.current_branch(self.run)?);
        self.record.source_commit = Some(self.effects.head_commit(self.run)?);
        self.record.identities = Vec::new();
        self.record.artifacts = Vec::new();
        self.record.verifications = Vec::new();
        self.record.errors.clear();
        self.record.checkpoints.clear();
        self.record.approvals.clear();
        self.record.decisions.clear();
        self.record.pushed_commit = false;
        self.record.targets = self
            .plan
            .targets
            .iter()
            .map(|target| RecordTarget {
                name: target.name.clone(),
                kind: target.kind.label().to_owned(),
                state: RecordTargetState::Pending,
                destinations: Vec::new(),
                note: None,
            })
            .collect();

        let identities = self.plan.identities.clone();
        for identity in identities {
            let mut recorded = RecordIdentity {
                name: identity.name.clone(),
                kind: identity.kind.label().to_owned(),
                identity: identity
                    .next_version
                    .as_ref()
                    .map(ToString::to_string)
                    .or_else(|| identity.identity_value.clone()),
                tag: identity.tag.clone(),
            };
            if identity.kind == IdentityKind::Task {
                let task = identity.task.clone().ok_or_else(|| {
                    ReleaseError::TaskInvocation(format!(
                        "release identity `{}` is kind `task` and names no task",
                        identity.name
                    ))
                })?;
                let outcome = self.run_release_task(
                    &task,
                    ReleaseStage::Validate,
                    ReleaseTaskSubject::Identity(&identity.name),
                )?;
                let value = outcome
                    .response
                    .stdout
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_owned();
                if value.is_empty() {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release identity `{}` ran task `{task}` and it printed no identity",
                        identity.name
                    )));
                }
                recorded.identity = Some(value);
            }
            if identity.kind == IdentityKind::Digest {
                recorded.identity = None;
            }
            self.record.identities.push(recorded);
        }
        self.persist()
    }

    fn run_gates(&mut self, stage: GateStage) -> Result<(), ReleaseError> {
        let gates: Vec<ResolvedGate> = self
            .plan
            .gates
            .iter()
            .filter(|gate| gate.stage == stage)
            .cloned()
            .collect();
        if gates.is_empty() {
            self.messages
                .push(format!("no `{}` gates configured", stage.stage_name()));
            return Ok(());
        }
        let mut failed = Vec::new();
        for gate in gates {
            if self.record.gate_passed(stage.stage_name(), &gate.name) {
                // A resumed gate stage continues where it stopped: a gate that
                // already passed is never re-run (contract 054 §3).
                self.messages.push(format!(
                    "gate `{}` already passed; it is not run again",
                    gate.name
                ));
                continue;
            }
            if let Some(spec) = gate.hosted_evidence.clone() {
                let commit = self.hosted_evidence_commit(stage)?;
                let binding = self.open_hosted_wait(&gate.name, &spec, &commit)?;
                match self
                    .effects
                    .hosted_evidence(self.run, &gate.name, &spec, &commit, &binding)?
                {
                    HostedEvidence::Accepted {
                        repository,
                        commit,
                        run,
                    } => {
                        // Bind and journal the one run this release dispatched,
                        // so a resume re-checks it instead of a sibling.
                        self.record
                            .bind_hosted_run(&gate.name, run.run_id, &run.url);
                        self.record.gates.push(RecordGate {
                            stage: stage.stage_name().to_owned(),
                            name: gate.name.clone(),
                            passed: true,
                            hosted_run: Some(run.url.clone()),
                            hosted_run_id: run.run_id,
                        });
                        self.messages.push(format!(
                            "gate `{}` reused hosted evidence {} for {repository}@{commit}",
                            gate.name,
                            match run.run_id {
                                Some(id) => format!("run #{id} {}", run.url),
                                None => run.url.clone(),
                            }
                        ));
                        continue;
                    }
                    HostedEvidence::Waiting { detail, run } => {
                        // The Run holds no process open: a resume re-checks the
                        // bound run and continues this same gate stage.
                        if let Some(run) = &run {
                            self.record
                                .bind_hosted_run(&gate.name, run.run_id, &run.url);
                        }
                        let release_id = self.record.release_id.clone();
                        self.wait_for(
                            ReleaseRunStatus::AwaitingHostedEvidence,
                            Some(gate.name.clone()),
                            format!(
                                "release Run {release_id} is waiting on hosted evidence for gate `{}`: {detail} `effigy release resume {release_id}` re-checks it without holding a process open",
                                gate.name
                            ),
                        );
                        return Ok(());
                    }
                    HostedEvidence::Failed { detail, run } => {
                        if let Some(run) = &run {
                            self.record
                                .bind_hosted_run(&gate.name, run.run_id, &run.url);
                        }
                        self.record.gates.push(RecordGate {
                            stage: stage.stage_name().to_owned(),
                            name: gate.name.clone(),
                            passed: false,
                            hosted_run: run.as_ref().map(|run| run.url.clone()),
                            hosted_run_id: run.as_ref().and_then(|run| run.run_id),
                        });
                        return Err(ReleaseError::TaskInvocation(detail));
                    }
                }
            }
            let report = run_release_gates_with_observer(
                &self.plan.repo_root,
                std::slice::from_ref(&gate),
                true,
                self.observer,
            );
            for result in &report.results {
                self.record.gates.push(RecordGate {
                    stage: stage.stage_name().to_owned(),
                    name: result.name.clone(),
                    passed: result.passed,
                    hosted_run: None,
                    hosted_run_id: None,
                });
                if result.passed {
                    continue;
                }
                let detail = if result.stderr.is_empty() {
                    result.stdout.clone()
                } else {
                    result.stderr.clone()
                };
                failed.push(format!("gate `{}` failed: {detail}", result.name));
            }
        }
        if failed.is_empty() {
            Ok(())
        } else {
            Err(ReleaseError::TaskInvocation(format!(
                "release {} gates failed: {}",
                stage.stage_name(),
                failed.join("; ")
            )))
        }
    }

    /// The hosting binding for a gate's evidence: the dispatch window this Run
    /// opened, and the run it already bound.
    ///
    /// The window opens the first time the gate needs evidence and is durable
    /// from then on, so a later dispatch cannot be mistaken for one that ran
    /// before this Run was waiting, and `effigy release resume` re-checks the
    /// same evidence instead of rebinding.
    fn open_hosted_wait(
        &mut self,
        gate: &str,
        spec: &HostedEvidenceSpec,
        commit: &str,
    ) -> Result<HostedEvidenceBinding, ReleaseError> {
        let fresh = self
            .record
            .hosted_wait(gate)
            .is_none_or(|wait| wait.commit != commit);
        let opened = self
            .record
            .open_hosted_wait(gate, &spec.workflow, commit, &now());
        let binding = HostedEvidenceBinding {
            created_after: Some(opened.created_after.clone()),
            run_id: opened.run_id,
        };
        if fresh {
            self.messages.push(format!(
                "gate `{gate}` opened its hosted dispatch window at {}: only a `{}` run created after it is this release's evidence",
                binding.created_after.as_deref().unwrap_or(""),
                spec.workflow
            ));
            self.persist()?;
        }
        Ok(binding)
    }

    /// The commit a hosted gate's evidence must be for: the candidate once
    /// `prepare` committed it, and the source commit before that.
    fn hosted_evidence_commit(&self, stage: GateStage) -> Result<String, ReleaseError> {
        let commit = match stage {
            GateStage::Pre => self.record.source_commit.clone(),
            GateStage::Certify => self
                .record
                .candidate_commit
                .clone()
                .or_else(|| self.record.source_commit.clone()),
        };
        commit.ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release hosted evidence for the `{}` gate stage has no commit: the validate stage records the source commit",
                stage.stage_name()
            ))
        })
    }

    fn prepare(&mut self) -> Result<(), ReleaseError> {
        let source = self.record.source_commit.clone().ok_or_else(|| {
            ReleaseError::TaskInvocation(
                "release prepare has no source commit; the validate stage records it".into(),
            )
        })?;
        let mutations = self.prepare_mutations()?;
        if mutations.is_empty() {
            // A release with no version file keeps the commit it validated.
            self.record.candidate_commit = Some(source);
            return self.persist();
        }

        let snapshots = crate::snapshot_mutation_paths(&mutations)?;
        self.prepared = snapshots
            .iter()
            .map(|(path, before)| (path.clone(), before.clone()))
            .collect();
        if let Err(error) = crate::apply_release_mutations(&self.plan.repo_root, &mutations) {
            let unrestored = self.rollback_prepared();
            return Err(ReleaseError::TaskInvocation(format!(
                "release prepare failed and the working tree was rolled back: {error}{}",
                rollback_note(&unrestored)
            )));
        }

        for mutation in &mutations {
            self.messages.push(format!(
                "prepared {}: {}",
                mutation.path.display(),
                mutation.summary
            ));
        }
        let versions: Vec<(String, String)> = self
            .semver_identities()
            .iter()
            .filter_map(|identity| {
                Some((
                    identity.name.clone(),
                    identity.next_version.as_ref()?.to_string(),
                ))
            })
            .collect();
        let message = prepare_commit_message(&versions);
        let paths: Vec<PathBuf> = mutations
            .iter()
            .map(|mutation| mutation.path.clone())
            .collect();
        match self.effects.commit_paths(self.run, &paths, &message) {
            Ok(commit) => {
                self.record.candidate_commit = Some(commit);
                self.persist()
            }
            Err(error) => {
                let unrestored = self.rollback_prepared();
                Err(ReleaseError::TaskInvocation(format!(
                    "release prepare could not commit the prepared tree: {error}{}",
                    rollback_note(&unrestored)
                )))
            }
        }
    }

    /// Restore the prepared files and unstage them: a failed prepare leaves the
    /// pre-prepare tree behind (contract 054 §3).
    fn rollback_prepared(&mut self) -> Vec<PathBuf> {
        let prepared = std::mem::take(&mut self.prepared);
        let restore: Vec<PathBuf> = prepared.iter().map(|(path, _)| path.clone()).collect();
        let snapshots: BTreeMap<PathBuf, Option<Vec<u8>>> = prepared.into_iter().collect();
        let unrestored = crate::restore_mutation_snapshots(&snapshots);
        if let Err(error) = self.effects.discard_staged(self.run, &restore) {
            self.messages.push(format!(
                "prepared release changes could not be unstaged: {error}"
            ));
        }
        unrestored
    }

    fn prepare_mutations(&mut self) -> Result<Vec<FileMutationPlan>, ReleaseError> {
        let mut mutations = Vec::new();
        for identity in self.plan.identities.clone() {
            if identity.kind != IdentityKind::Semver {
                continue;
            }
            let (Some(source), Some(next)) = (&identity.version_source, &identity.next_version)
            else {
                continue;
            };
            if identity.current_version.as_ref() == Some(next) {
                continue;
            }
            let before = std::fs::read_to_string(&source.path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read release version file {}: {error}",
                    source.path.display()
                ))
            })?;
            let after = render_updated_version_contents(source, next)?;
            mutations.push(version_write_mutation(
                &source.path,
                &before,
                &after,
                identity.current_version.as_ref(),
                next,
            ));
        }

        let semver = self.semver_identities();
        let Some(primary) = semver
            .first()
            .and_then(|identity| identity.next_version.clone())
        else {
            return Ok(mutations);
        };

        if self.plan.changelog_in_use {
            let raw = std::fs::read_to_string(&self.plan.changelog_path).map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to read changelog {}: {error}",
                    self.plan.changelog_path.display()
                ))
            })?;
            let parsed = changelog::parse(&raw)
                .map_err(|error| ReleaseError::TaskInvocation(error.to_string()))?;
            let date = Utc::now().date_naive().to_string();
            let after = render_prepared_changelog_contents(&parsed, &primary, &date)?;
            if after != raw {
                mutations.push(changelog_write_mutation(
                    &self.plan.changelog_path,
                    &raw,
                    &after,
                    &primary,
                    &date,
                ));
            }
        }

        let sync_files = self.plan_sync_files();
        if !sync_files.is_empty() {
            for identity in semver.iter().skip(1) {
                if identity.next_version.as_ref() != Some(&primary) {
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release identities `{}` and `{}` have different next versions ({primary} and {}), and the release declares `sync-files`; a lockfile carries one version, so remove `release.sync-files` or give the identities the same version with `follows`",
                        semver[0].name,
                        identity.name,
                        identity
                            .next_version
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "none".to_owned())
                    )));
                }
            }
            mutations.extend(build_sync_mutations(
                &self.plan.repo_root,
                &sync_files,
                &primary,
            )?);
        }
        Ok(mutations)
    }

    fn semver_identities(&self) -> Vec<ReleaseIdentityPlan> {
        self.plan
            .identities
            .iter()
            .filter(|identity| identity.kind == IdentityKind::Semver)
            .cloned()
            .collect()
    }

    /// The release-level sync files, which never repeat an identity's version
    /// file.
    fn plan_sync_files(&self) -> Vec<ResolvedSyncFile> {
        self.plan
            .files
            .iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == "Cargo.lock" || name == "package.json")
            })
            .filter(|path| {
                !self.plan.identities.iter().any(|identity| {
                    identity
                        .version_source
                        .as_ref()
                        .is_some_and(|source| &source.path == *path)
                })
            })
            .filter(|path| path != &&self.plan.changelog_path)
            .map(|path| ResolvedSyncFile {
                path: path.clone(),
                kind: if path.file_name().and_then(|name| name.to_str()) == Some("Cargo.lock") {
                    crate::SyncFileKind::CargoLock
                } else {
                    crate::SyncFileKind::PackageJson
                },
            })
            .collect()
    }

    fn certify(&mut self) -> Result<(), ReleaseError> {
        self.run_gates(GateStage::Certify)?;
        if self.waiting.is_some() {
            // A hosted wait stopped the gate stage: the Run captures no
            // artifacts until the gate has its exact-commit evidence (054 §3).
            return Ok(());
        }
        let candidate = self.record.candidate_commit.clone().ok_or_else(|| {
            ReleaseError::TaskInvocation(
                "release certify has no candidate commit; the prepare stage records it".into(),
            )
        })?;

        let artifacts = self.plan.artifacts.clone();
        let mut captured = Vec::new();
        for artifact in artifacts {
            let outcome = self.run_release_task(
                &artifact.task,
                ReleaseStage::Certify,
                ReleaseTaskSubject::Artifact(&artifact.name),
            )?;
            if !outcome.response.success {
                return Err(ReleaseError::TaskInvocation(format!(
                    "release artifact `{}` task `{}` failed: {}",
                    artifact.name,
                    artifact.task,
                    if outcome.response.stderr.is_empty() {
                        outcome.response.stdout
                    } else {
                        outcome.response.stderr
                    }
                )));
            }
            let recorded = capture_artifact(
                &self.plan.repo_root,
                self.candidate_dir,
                &artifact.name,
                &artifact.task,
                &artifact.path,
            )?;
            self.messages.push(format!(
                "certified artifact `{}` from `{}`: {} file(s)",
                artifact.name,
                artifact.path,
                recorded.files.len()
            ));
            captured.push(recorded);
        }
        self.record.artifacts = captured;

        for index in 0..self.record.identities.len() {
            match parse_identity_kind(&self.record.identities[index].kind) {
                Some(IdentityKind::Digest) => {
                    let name = self.record.identities[index].name.clone();
                    let digest = self
                        .record
                        .artifacts
                        .iter()
                        .find(|artifact| artifact.name == name)
                        .and_then(RecordArtifact::identity_digest);
                    if digest.is_none() {
                        return Err(ReleaseError::TaskInvocation(format!(
                            "release identity `{name}` is kind `digest` and its artifact recorded no file"
                        )));
                    }
                    self.record.identities[index].identity = digest;
                }
                Some(IdentityKind::Source) => {
                    self.record.identities[index].identity = Some(candidate.clone());
                }
                _ => {}
            }
        }
        self.persist()
    }

    fn publish(&mut self) -> Result<(), ReleaseError> {
        verify_artifact_digests(&self.record.artifacts)?;
        let candidate = self.record.candidate_commit.clone().ok_or_else(|| {
            ReleaseError::TaskInvocation(
                "release publish has no candidate commit; the prepare stage records it".into(),
            )
        })?;
        // Targets publish in declared order. A target that succeeded stays
        // recorded, so a resume continues with the remaining ones (054 §7).
        for target in self.plan.targets.clone() {
            let state = self
                .record
                .target(target.name.as_str())
                .map(|recorded| recorded.state)
                .unwrap_or(RecordTargetState::Pending);
            if state == RecordTargetState::Published {
                continue;
            }
            if state == RecordTargetState::Unknown && target.kind == TargetKind::Task {
                match self.reconcile_task_target(&target)? {
                    Reconcile::Published { destinations, note } => {
                        self.mark_target_published(&target.name, destinations, note, &candidate)?;
                        continue;
                    }
                    Reconcile::NotPublished => {
                        self.set_target_state(&target.name, RecordTargetState::Pending)?;
                    }
                    Reconcile::Waiting => return Ok(()),
                }
            }
            match target.kind {
                TargetKind::Tag => self.publish_tags(&target.name, &candidate)?,
                TargetKind::Task => self.publish_task(&target, &candidate)?,
            }
            if self.waiting.is_some() {
                // The wait stops the fan-out: no later target publishes until
                // the operator has ruled on this one (054 §7).
                return Ok(());
            }
        }
        self.persist()
    }

    /// A `task` target publishes the certified candidate through a project
    /// task and reads back what it reported (contract 054 §6, §7).
    fn publish_task(
        &mut self,
        target: &crate::pipeline::ResolvedTarget,
        candidate: &str,
    ) -> Result<(), ReleaseError> {
        let task = target.task.clone().ok_or_else(|| {
            ReleaseError::TaskInvocation(format!(
                "release target `{}` is kind `task` and names no task",
                target.name
            ))
        })?;
        // Write-ahead: from the moment the task starts, this target's outcome
        // is unknown until the task reports one. A crash mid-task therefore
        // leaves `Unknown`, and a resume reconciles with `check` or an operator
        // decision instead of publishing again blind (054 §7).
        self.set_target_state(&target.name, RecordTargetState::Unknown)?;
        let ticket = self.intend_irreversible(
            kernel::TASK_PUBLISH_EFFECT,
            serde_json::json!({
                "target": target.name,
                "task": task,
                "commit": candidate,
            }),
        )?;
        let outcome = match self.run_release_task(
            &task,
            ReleaseStage::Publish,
            ReleaseTaskSubject::Target(&target.name),
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.record_unknown(&ticket, &error)?;
                return self
                    .stop_after_unknown(target, &format!("task `{task}` could not run: {error}"));
            }
        };
        match task_verdict(&outcome, task.as_str()) {
            TaskVerdict::Committed { destinations, note } => {
                self.record_applied(&ticket, &target.name, candidate)?;
                self.messages.push(format!(
                    "release target `{}` published through task `{task}`{}",
                    target.name,
                    match destinations.as_slice() {
                        [] => String::new(),
                        destinations => format!(": {}", destinations.join(", ")),
                    }
                ));
                self.mark_target_published(&target.name, destinations, note, candidate)
            }
            TaskVerdict::NotCommitted { note } => {
                self.record_applied(&ticket, &target.name, candidate)?;
                Err(self.target_failed(
                    &target.name,
                    format!(
                        "release target `{}` task `{task}` reported it did not publish the candidate{}{}; nothing is recorded as published and the release stops before later targets",
                        target.name,
                        note_suffix(note.as_deref()),
                        stderr_suffix(&outcome.response)
                    ),
                ))
            }
            TaskVerdict::Unknown { detail } => {
                self.record_unknown(&ticket, &ReleaseError::TaskInvocation(detail.clone()))?;
                self.stop_after_unknown(
                    target,
                    &format!(
                        "task `{task}` outcome is unknown: {detail}{}",
                        stderr_suffix(&outcome.response)
                    ),
                )
            }
        }
    }

    /// After an unknown publish outcome: a target that declares a `check`
    /// fails here so the resume reconciles it first, and a target with no
    /// `check` stops this invocation for the operator's ruling (054 §7).
    fn stop_after_unknown(
        &mut self,
        target: &crate::pipeline::ResolvedTarget,
        detail: &str,
    ) -> Result<(), ReleaseError> {
        if target.check.is_some() {
            return Err(self.target_unknown(
                &target.name,
                format!(
                    "release target `{}` {detail}; `effigy release resume {}` runs its `check` task before any retry",
                    target.name, self.record.release_id
                ),
            ));
        }
        let release_id = self.record.release_id.clone();
        self.messages
            .push(format!("release target `{}` {detail}", target.name));
        self.wait_for(
            ReleaseRunStatus::AwaitingDecision,
            Some(target.name.clone()),
            format!(
                "release Run {release_id} stopped at `awaiting-decision`: target `{}` {detail} and declares no `check` task, so the operator records the outcome with `effigy release decide {release_id} --published` or `--not-published`",
                target.name
            ),
        );
        Ok(())
    }

    /// Reconcile a `task` target whose last publish outcome is unknown.
    ///
    /// The target's `check` task reports whether this candidate is already at
    /// the destination. With no `check`, the operator decides (054 §7).
    fn reconcile_task_target(
        &mut self,
        target: &crate::pipeline::ResolvedTarget,
    ) -> Result<Reconcile, ReleaseError> {
        let Some(check) = target.check.clone() else {
            let release_id = self.record.release_id.clone();
            if let Some(recorded) = self.record.target_mut(&target.name) {
                recorded.note = Some(
                    "publish outcome unknown; the operator records published or not published"
                        .to_owned(),
                );
            }
            self.wait_for(
                ReleaseRunStatus::AwaitingDecision,
                Some(target.name.clone()),
                format!(
                    "release Run {release_id} stopped at `awaiting-decision`: target `{}` declares no `check` task, so its unknown publish outcome needs an operator ruling; record it with `effigy release decide {release_id} --published` or `--not-published`",
                    target.name
                ),
            );
            return Ok(Reconcile::Waiting);
        };
        let outcome = self.run_release_task(
            &check,
            ReleaseStage::Publish,
            ReleaseTaskSubject::Target(&target.name),
        )?;
        Ok(match task_verdict(&outcome, check.as_str()) {
            TaskVerdict::Committed { destinations, note } => {
                self.messages.push(format!(
                    "release target `{}` check task `{check}` reports the candidate is already published",
                    target.name
                ));
                Reconcile::Published { destinations, note }
            }
            TaskVerdict::NotCommitted { note } => {
                self.messages.push(format!(
                    "release target `{}` check task `{check}` reports the candidate is not published{f}; publishing it now",
                    target.name,
                    f = note_suffix(note.as_deref()),
                ));
                Reconcile::NotPublished
            }
            TaskVerdict::Unknown { detail } => {
                let release_id = self.record.release_id.clone();
                self.wait_for(
                    ReleaseRunStatus::AwaitingDecision,
                    Some(target.name.clone()),
                    format!(
                        "release Run {release_id} stopped at `awaiting-decision`: target `{}` check task `{check}` could not tell whether the candidate is published ({detail}); record it with `effigy release decide {release_id} --published` or `--not-published`",
                        target.name
                    ),
                );
                Reconcile::Waiting
            }
        })
    }

    fn mark_target_published(
        &mut self,
        target: &str,
        destinations: Vec<String>,
        note: Option<String>,
        candidate: &str,
    ) -> Result<(), ReleaseError> {
        for destination in &destinations {
            self.record_destination(target, None, destination, candidate)?;
        }
        if let Some(recorded) = self.record.target_mut(target) {
            recorded.state = RecordTargetState::Published;
            if note.is_some() {
                recorded.note = note;
            }
        }
        self.persist()
    }

    fn set_target_state(
        &mut self,
        target: &str,
        state: RecordTargetState,
    ) -> Result<(), ReleaseError> {
        if let Some(recorded) = self.record.target_mut(target) {
            recorded.state = state;
        }
        self.persist()
    }

    fn publish_tags(&mut self, target: &str, candidate: &str) -> Result<(), ReleaseError> {
        let tags: Vec<(String, String)> = self
            .record
            .identities
            .iter()
            .filter_map(|identity| Some((identity.name.clone(), identity.tag.clone()?)))
            .collect();
        if tags.is_empty() {
            return Err(ReleaseError::TaskInvocation(format!(
                "release target `{target}` is kind `tag` and the release has no identity with a `tag-format`"
            )));
        }
        for (identity, tag) in tags {
            let already = self.record.target(target).is_some_and(|recorded| {
                recorded
                    .destinations
                    .iter()
                    .any(|destination| destination.destination == tag)
            });
            if already {
                continue;
            }
            // Reconcile before any retry: an interrupted invocation may have
            // pushed this tag without recording it (contract 054 §7).
            match self.effects.remote_tag_commit(self.run, &tag) {
                Ok(Some(sha)) if sha == candidate => {
                    self.record_destination(target, Some(&identity), &tag, candidate)?;
                    continue;
                }
                Ok(Some(sha)) => {
                    return Err(self.target_failed(
                        target,
                        format!(
                            "release tag `{tag}` already exists on the remote at {sha}, not at the candidate commit {candidate}; a published tag is never moved"
                        ),
                    ))
                }
                Ok(None) => {}
                Err(error) => {
                    return Err(self.target_unknown(
                        target,
                        format!(
                            "release tag `{tag}` could not be looked up on the remote: {error}; its publish outcome is unknown"
                        ),
                    ))
                }
            }

            match self.effects.local_tag_commit(self.run, &tag) {
                Ok(Some(sha)) if sha == candidate => {}
                Ok(Some(sha)) => {
                    return Err(self.target_failed(
                        target,
                        format!(
                            "release tag `{tag}` exists locally at {sha}, not at the candidate commit {candidate}; delete it before re-running"
                        ),
                    ))
                }
                Ok(None) => {
                    let message = ANNOTATED_TAG_MESSAGE.replace("{tag}", &tag);
                    self.effects.create_annotated_tag(self.run, &tag, candidate, &message)?;
                }
                Err(error) => {
                    return Err(self.target_failed(
                        target,
                        format!("release tag `{tag}` could not be read locally: {error}"),
                    ))
                }
            }

            let ticket = self.intend_irreversible(
                kernel::TAG_PUSH_EFFECT,
                serde_json::json!({ "tag": tag, "commit": candidate }),
            )?;
            match self.effects.push_tag(self.run, &tag) {
                Ok(()) => {
                    self.record_applied(&ticket, &tag, candidate)?;
                    self.record_destination(target, Some(&identity), &tag, candidate)?;
                }
                Err(error) => {
                    self.record_unknown(&ticket, &error)?;
                    return Err(self.target_unknown(
                        target,
                        format!(
                            "release tag `{tag}` push outcome is unknown: {error}; `effigy release resume {}` reconciles it before any retry",
                            self.record.release_id
                        ),
                    ));
                }
            }
        }
        if let Some(recorded) = self
            .record
            .targets
            .iter_mut()
            .find(|recorded| recorded.name == target)
        {
            recorded.state = RecordTargetState::Published;
        }
        self.persist()
    }

    fn target_failed(&mut self, target: &str, error: String) -> ReleaseError {
        if let Some(recorded) = self
            .record
            .targets
            .iter_mut()
            .find(|recorded| recorded.name == target)
        {
            recorded.state = RecordTargetState::Failed;
        }
        let _ = self.persist();
        ReleaseError::TaskInvocation(error)
    }

    fn target_unknown(&mut self, target: &str, error: String) -> ReleaseError {
        if let Some(recorded) = self
            .record
            .targets
            .iter_mut()
            .find(|recorded| recorded.name == target)
        {
            recorded.state = RecordTargetState::Unknown;
        }
        let _ = self.persist();
        ReleaseError::TaskInvocation(error)
    }

    fn record_destination(
        &mut self,
        target: &str,
        identity: Option<&str>,
        destination: &str,
        commit: &str,
    ) -> Result<(), ReleaseError> {
        if let Some(recorded) = self.record.target_mut(target) {
            recorded.destinations.push(RecordDestination {
                identity: identity.map(str::to_owned),
                destination: destination.to_owned(),
                commit: commit.to_owned(),
            });
        }
        self.persist()
    }

    fn verify(&mut self) -> Result<(), ReleaseError> {
        // Verification writes nothing external, so a resumed verify stage
        // re-runs it and records one result per target rather than appending a
        // second copy of each.
        self.record.verifications.clear();
        let mut failed = Vec::new();
        for target in self.plan.targets.clone() {
            let Some(task) = target.verify.clone() else {
                continue;
            };
            let outcome = self.run_release_task(
                &task,
                ReleaseStage::Verify,
                ReleaseTaskSubject::Target(&target.name),
            );
            let (passed, note) = match &outcome {
                Ok(outcome) => (
                    outcome.response.success,
                    (!outcome.response.success).then(|| {
                        if outcome.response.stderr.is_empty() {
                            outcome.response.stdout.clone()
                        } else {
                            outcome.response.stderr.clone()
                        }
                    }),
                ),
                Err(error) => (false, Some(error.to_string())),
            };
            self.record.verifications.push(RecordVerification {
                target: target.name.clone(),
                task: task.clone(),
                passed,
                note,
            });
            if !passed {
                failed.push(format!(
                    "release verify task `{task}` for target `{}` failed",
                    target.name
                ));
            }
        }
        if !failed.is_empty() {
            // Published but unverified: the receipt records the failure and
            // closeout still pushes the release commit (contract 054 §7).
            self.record.errors.push(format!(
                "{}; the release is published but unverified",
                failed.join("; ")
            ));
            self.messages.push(format!(
                "release Run {} is published but unverified; the receipt records the failure",
                self.record.release_id
            ));
        }
        self.persist()
    }

    fn closeout(&mut self) -> Result<(), ReleaseError> {
        if !self.record.pushed_commit {
            let commit = self.record.candidate_commit.clone().ok_or_else(|| {
                ReleaseError::TaskInvocation(
                    "release closeout has no candidate commit to push; the prepare stage records it"
                        .into(),
                )
            })?;
            let branch = self.record.branch.clone().ok_or_else(|| {
                ReleaseError::TaskInvocation(
                    "release closeout has no branch to push; the validate stage records it".into(),
                )
            })?;
            let ticket = self.intend_irreversible(
                kernel::COMMIT_PUSH_EFFECT,
                serde_json::json!({ "commit": commit, "branch": branch }),
            )?;
            match self.effects.push_commit(self.run, &commit, &branch) {
                Ok(()) => {
                    self.record_applied(&ticket, &branch, &commit)?;
                    self.record.pushed_commit = true;
                }
                Err(error) => {
                    self.record_unknown(&ticket, &error)?;
                    return Err(ReleaseError::TaskInvocation(format!(
                        "release commit {commit} push outcome is unknown: {error}; `effigy release resume {}` reconciles it",
                        self.record.release_id
                    )));
                }
            }
        }
        self.persist()?;
        self.write_receipt()
    }

    fn write_receipt(&mut self) -> Result<(), ReleaseError> {
        let receipt = ReleaseReceipt::from_record(self.record);
        let bytes = serde_json::to_vec_pretty(&receipt).map_err(|error| {
            ReleaseError::TaskInvocation(format!("failed to encode the release receipt: {error}"))
        })?;
        let path = self.state.record_path.with_file_name(RELEASE_RECEIPT_FILE);
        std::fs::write(&path, bytes).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to write the release receipt {}: {error}",
                path.display()
            ))
        })
    }

    fn intend_irreversible(
        &mut self,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<EffectTicket, ReleaseError> {
        let target = self.run.fence().clone();
        self.run
            .intend(effigy_kernel::run::EffectSpec {
                target,
                kind: kind.to_owned(),
                payload,
                settlement: Settlement::Irreversible,
                required: false,
            })
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to journal the release effect `{kind}`: {error}"
                ))
            })
    }

    fn record_applied(
        &mut self,
        ticket: &EffectTicket,
        destination: &str,
        commit: &str,
    ) -> Result<(), ReleaseError> {
        self.run
            .applied(
                ticket,
                serde_json::json!({ "destination": destination, "commit": commit }),
            )
            .map_err(|error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to record the published destination `{destination}`: {error}"
                ))
            })
    }

    fn record_unknown(
        &mut self,
        ticket: &EffectTicket,
        error: &ReleaseError,
    ) -> Result<(), ReleaseError> {
        self.run
            .unknown(ticket, UnknownReason::Inconsistent(error.to_string()))
            .map_err(|kernel_error| {
                ReleaseError::TaskInvocation(format!(
                    "failed to record an unknown publish outcome: {kernel_error}"
                ))
            })
    }

    fn run_release_task(
        &mut self,
        task: &str,
        stage: ReleaseStage,
        subject: ReleaseTaskSubject<'_>,
    ) -> Result<TaskOutcome, ReleaseError> {
        let name = subject.name().unwrap_or(task);
        let stem = format!("{}-{}", sanitize(name), stage.label());
        let input_path = self.candidate_dir.join(format!("{stem}.input.json"));
        let output_path = self.candidate_dir.join(format!("{stem}.output.json"));
        // A stale output from an earlier attempt must never be read as this
        // attempt's answer.
        match std::fs::remove_file(&output_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ReleaseError::TaskInvocation(format!(
                    "failed to clear the previous release task output {}: {error}",
                    output_path.display()
                )))
            }
        }
        let input = input::release_input_value(self.record, stage, subject);
        let bytes = serde_json::to_vec_pretty(&input).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to encode the release task input for `{name}`: {error}"
            ))
        })?;
        std::fs::write(&input_path, bytes).map_err(|error| {
            ReleaseError::TaskInvocation(format!(
                "failed to write the release task input {}: {error}",
                input_path.display()
            ))
        })?;
        let mut env = BTreeMap::new();
        env.insert(
            input::RELEASE_INPUT_ENV.to_owned(),
            input_path.display().to_string(),
        );
        env.insert(
            input::RELEASE_OUTPUT_ENV.to_owned(),
            output_path.display().to_string(),
        );
        env.insert(
            "EFFIGY_RELEASE_CANDIDATE_DIR".to_owned(),
            self.candidate_dir.display().to_string(),
        );
        env.insert(
            "EFFIGY_RELEASE_RUN".to_owned(),
            self.record.release_id.clone(),
        );
        env.insert("EFFIGY_RELEASE_STAGE".to_owned(), stage.label().to_owned());
        let response = self.effects.run_task(
            self.run,
            &TaskRequest {
                task: task.to_owned(),
                label: format!("release {} task `{task}`", stage.label()),
                cwd: self.plan.repo_root.clone(),
                env,
                input_path,
                output_path: output_path.clone(),
            },
        )?;
        let output = input::read_release_task_output(&output_path);
        Ok(TaskOutcome { response, output })
    }
}

/// One task invocation and what it wrote to `$EFFIGY_RELEASE_OUTPUT`.
struct TaskOutcome {
    response: TaskResponse,
    /// `Err` is an unusable output document, never a guess at its meaning.
    output: Result<Option<ReleaseTaskOutput>, String>,
}

/// What a task target's result means for the destination.
enum TaskVerdict {
    Committed {
        destinations: Vec<String>,
        note: Option<String>,
    },
    NotCommitted {
        note: Option<String>,
    },
    Unknown {
        detail: String,
    },
}

/// Read a task target's declared result.
///
/// Contract 054 §6: a task that exits 0 without writing an output records
/// `committed = unknown`, and an unusable output is an unknown outcome too — it
/// never becomes "published" or "not published" by assumption.
fn task_verdict(outcome: &TaskOutcome, task: &str) -> TaskVerdict {
    let output = match &outcome.output {
        Ok(Some(output)) => output,
        Ok(None) if outcome.response.success => {
            return TaskVerdict::Unknown {
                detail: format!("task `{task}` exited 0 without writing an output"),
            }
        }
        Ok(None) => {
            return TaskVerdict::Unknown {
                detail: format!("task `{task}` failed without writing an output"),
            }
        }
        Err(reason) => {
            return TaskVerdict::Unknown {
                detail: format!("task `{task}` output is unusable: {reason}"),
            }
        }
    };
    match output.committed {
        Committed::Yes => TaskVerdict::Committed {
            destinations: output.destinations.clone(),
            note: output.note.clone(),
        },
        Committed::No => TaskVerdict::NotCommitted {
            note: output.note.clone(),
        },
        Committed::Unknown => TaskVerdict::Unknown {
            detail: match &output.note {
                Some(note) => format!("task `{task}` reported committed = unknown: {note}"),
                None => format!("task `{task}` reported committed = unknown"),
            },
        },
    }
}

/// What reconciling an unknown publish found.
enum Reconcile {
    Published {
        destinations: Vec<String>,
        note: Option<String>,
    },
    NotPublished,
    /// The Run stopped for an operator decision.
    Waiting,
}

fn note_suffix(note: Option<&str>) -> String {
    match note {
        Some(note) => format!(" ({note})"),
        None => String::new(),
    }
}

fn stderr_suffix(response: &TaskResponse) -> String {
    if response.stderr.is_empty() {
        String::new()
    } else {
        format!(": {}", response.stderr)
    }
}

fn version_write_mutation(
    path: &Path,
    before: &str,
    after: &str,
    current: Option<&semver::Version>,
    next: &semver::Version,
) -> FileMutationPlan {
    FileMutationPlan {
        path: path.to_path_buf(),
        kind: "version-file",
        summary: match current {
            Some(current) => format!("update version from {current} to {next}"),
            None => format!("write version {next}"),
        },
        before_preview: format!(
            "version {}",
            current
                .map(ToString::to_string)
                .unwrap_or_else(|| "unknown".to_owned())
        ),
        after_preview: format!("version {next}"),
        detail_lines: vec![format!("{} carries the release version", path.display())],
        diff_preview: crate::build_diff_preview(before, after),
        apply: FileMutationApply::Write {
            after_contents: after.to_owned(),
        },
    }
}

fn changelog_write_mutation(
    path: &Path,
    before: &str,
    after: &str,
    version: &semver::Version,
    date: &str,
) -> FileMutationPlan {
    FileMutationPlan {
        path: path.to_path_buf(),
        kind: "changelog",
        summary: format!("promote [Unreleased] into [{version}] - {date} and reset [Unreleased]"),
        before_preview: "[Unreleased] holds the notes this release promotes".to_owned(),
        after_preview: format!("[{version}] - {date} added; [Unreleased] reset"),
        detail_lines: vec![
            format!("{} records the release notes", path.display()),
            "one changelog feeds every identity (contract 054 §4.2)".to_owned(),
        ],
        diff_preview: crate::build_diff_preview(before, after),
        apply: FileMutationApply::Write {
            after_contents: after.to_owned(),
        },
    }
}

fn parse_identity_kind(value: &str) -> Option<IdentityKind> {
    [
        IdentityKind::Semver,
        IdentityKind::Digest,
        IdentityKind::Source,
        IdentityKind::Task,
    ]
    .into_iter()
    .find(|kind| kind.label() == value.trim())
}

fn rollback_note(unrestored: &[PathBuf]) -> String {
    if unrestored.is_empty() {
        return "; the working tree is back at its pre-prepare state".to_owned();
    }
    let mut paths = unrestored
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    paths.sort();
    format!(
        "; these prepared files could not be restored and remain in the working tree: {}",
        paths.join(", ")
    )
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn prepare_commit_message(versions: &[(String, String)]) -> String {
    match versions {
        [] => "release: prepare".to_owned(),
        [(_, version)] => format!("release: v{version}"),
        versions => format!(
            "release: {}",
            versions
                .iter()
                .map(|(name, version)| format!("{name} {version}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}
