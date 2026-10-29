use std::path::Path;

use effigy_core::resolver::ResolvedTarget;
use effigy_release::{
    collect_release_gate_run, collect_release_status as collect_release_status_via_release,
    gate_outcome_word, plan_release_pipeline as plan_release_pipeline_via_release,
    release_home as release_home_via_release, release_receipt_for_run,
    run_release_gates_with_observer, run_release_with_observer, GateExecutionReport, GateObserver,
    GateResult, GitEffects, ReleaseAction, ReleaseGateRun, ReleasePipelinePlan, ReleaseRunOutcome,
    ReleaseRunRequest, ReleaseStage, ReleaseStatus, ResolvedGate,
};

use effigy_ui::steps::{live_step, outcome_line, ProgressSource, Step};

use super::*;

/// What a release Run waits on between gates and stages.
pub(super) const NEXT_GATE_OR_STAGE: &str = "the next gate or stage";

pub(super) fn collect_release_status(
    resolved: &ResolvedTarget,
    check_gates: bool,
) -> Result<ReleaseStatus, RunnerError> {
    // Status resolves the pipeline instead of the shorthand context, so a
    // release with no `semver` identity (contract 054 §4.3) still reports.
    let plan = plan_release_pipeline_via_release(&resolved.resolved_root)?;
    if check_gates && !plan.gates.is_empty() {
        emit_release_progress_line("checking release gates for status");
    }
    let gate_report = if check_gates {
        run_release_gates(&resolved.resolved_root, &plan.gates, true)
    } else {
        GateExecutionReport::empty()
    };
    Ok(collect_release_status_via_release(
        &plan,
        check_gates,
        gate_report,
    ))
}

pub(super) fn collect_release_plan(
    resolved: &ResolvedTarget,
    version_override: Option<semver::Version>,
) -> Result<ReleasePipelinePlan, RunnerError> {
    let mut plan = plan_release_pipeline_via_release(&resolved.resolved_root)?;
    if let Some(version) = version_override {
        apply_release_plan_version_override(&mut plan, &version)?;
    }
    Ok(plan)
}

/// Apply `release --plan --version` to the single `semver` identity.
///
/// The override is the successor of `release simulate --version`: it reports
/// what the pipeline would resolve to, and writes nothing. A release with more
/// than one `semver` identity refuses it, because one flag cannot say which
/// identity it means.
fn apply_release_plan_version_override(
    plan: &mut ReleasePipelinePlan,
    version: &semver::Version,
) -> Result<(), RunnerError> {
    let semver_identities = plan
        .identities
        .iter()
        .enumerate()
        .filter(|(_, identity)| identity.kind == effigy_release::IdentityKind::Semver)
        .map(|(index, identity)| (index, identity.name.clone()))
        .collect::<Vec<_>>();
    let [(index, name)] = semver_identities.as_slice() else {
        return Err(RunnerError::task_invocation(format!(
            "`release --plan --version` needs exactly one `semver` identity; this release declares {} ({})",
            semver_identities.len(),
            semver_identities
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let name = name.clone();
    let identity = &mut plan.identities[*index];
    let Some(current) = identity.current_version.clone() else {
        return Err(RunnerError::task_invocation(format!(
            "`release --plan --version` needs a changelog-derived next version for identity `{name}`"
        )));
    };
    // `== current` stays legal only when the plan already resolves to it: that
    // is the `initial-tag-current-version` release with no changelog history.
    let permits_current = identity.next_version.as_ref() == Some(&current);
    if version < &current || (version == &current && !permits_current) {
        return Err(RunnerError::task_invocation(format!(
            "invalid `release --plan --version`: {version} must be greater than current version {current}"
        )));
    }
    if plan
        .existing_changelog_versions
        .iter()
        .any(|existing| existing == &version.to_string())
    {
        return Err(RunnerError::task_invocation(format!(
            "invalid `release --plan --version`: the changelog already contains release version {version}"
        )));
    }
    identity.tag = identity
        .tag_format
        .as_deref()
        .map(|format| effigy_release::format_release_tag(format, version));
    identity.next_version = Some(version.clone());
    Ok(())
}

/// Run, resume or approve a release Run (contract 054 §3, §5).
pub(super) fn release_run_action(
    resolved: &ResolvedTarget,
    action: ReleaseAction,
    to: Option<ReleaseStage>,
) -> Result<ReleaseRunOutcome, RunnerError> {
    let home = release_home_via_release()?;
    let progress = ProgressSource::waiting_for(NEXT_GATE_OR_STAGE);
    progress.update(release_run_label(&action, to), None, None);
    let step = live_step(progress.clone());
    let action_label = action_name(&action);
    let request = ReleaseRunRequest {
        repo_root: resolved.resolved_root.clone(),
        home,
        action,
        to,
    };
    let mut effects = GitEffects::new(&resolved.resolved_root);
    let mut observer = ReleaseRunSteps {
        root: step.clone(),
        progress,
        gate: None,
    };
    let result = run_release_with_observer(&request, &mut effects, &mut observer);
    let outcome = match result {
        Ok(outcome) => {
            step.succeed(&release_run_summary(&outcome));
            outcome
        }
        Err(error) => {
            step.fail(&format!(
                "release {action_label} stopped: {}",
                effigy_ui::HumanError::new(error.to_string()).what_failed()
            ));
            return Err(error.into());
        }
    };
    for message in &outcome.messages {
        emit_release_progress_line(message);
    }
    Ok(outcome)
}

/// Draws a release Run's stages and gates on one live step. The root names the
/// stage, the gate now running and the last gate's outcome; each gate also has
/// its own live step, which names the exact command it waits on.
pub(super) struct ReleaseRunSteps {
    pub(super) root: Step,
    pub(super) progress: ProgressSource,
    pub(super) gate: Option<Step>,
}

impl Drop for ReleaseRunSteps {
    fn drop(&mut self) {
        if let Some(gate) = self.gate.take() {
            gate.finish_clear();
        }
    }
}

impl GateObserver for ReleaseRunSteps {
    fn gate_started(
        &mut self,
        position: usize,
        total: usize,
        gate: &ResolvedGate,
        reusing_hosted_evidence: bool,
    ) {
        if let Some(previous) = self.gate.take() {
            previous.finish_clear();
        }
        show_gate(
            &self.progress,
            position,
            total,
            gate,
            reusing_hosted_evidence,
        );
        self.gate = Some(self.root.live_step(gate_progress(
            position,
            total,
            gate,
            reusing_hosted_evidence,
        )));
    }

    fn gate_finished(&mut self, position: usize, total: usize, result: &GateResult) {
        let summary = release_gate_summary(position, total, result);
        match self.gate.take() {
            Some(step) if result.passed => step.succeed(&summary),
            Some(step) => step.fail(&summary),
            None => {}
        }
        self.progress.update(
            "checking the next gate or stage",
            Some((position as u64, total as u64)),
            Some(summary),
        );
        self.progress.waiting_on(NEXT_GATE_OR_STAGE);
    }

    fn stage(&mut self, stage: &str) {
        self.progress
            .update(format!("running stage {stage}"), None, None);
        self.progress.waiting_on(NEXT_GATE_OR_STAGE);
    }

    fn note(&mut self, message: &str) {
        self.progress.update(message, None, None);
        self.progress.waiting_on(NEXT_GATE_OR_STAGE);
    }
}

/// The live label while a release Run works: what this invocation is doing,
/// not which internal stage it is in.
fn release_run_label(action: &ReleaseAction, to: Option<ReleaseStage>) -> String {
    let mut label = match action {
        ReleaseAction::Run => "running the release".to_owned(),
        ReleaseAction::Resume { run } => format!("resuming release run {run}"),
        ReleaseAction::Approve { run } => format!("approving release run {run}"),
        ReleaseAction::Decide { run, .. } => {
            format!("recording the decision for release run {run}")
        }
    };
    if let Some(to) = to {
        label.push_str(&format!(" through {}", to.label()));
    }
    label
}

fn action_name(action: &ReleaseAction) -> &'static str {
    match action {
        ReleaseAction::Run => "run",
        ReleaseAction::Resume { .. } => "resume",
        ReleaseAction::Approve { .. } => "approve",
        ReleaseAction::Decide { .. } => "decide",
    }
}

/// One line when a release invocation ends: the stage it reached and whether
/// it is waiting for something.
fn release_run_summary(outcome: &ReleaseRunOutcome) -> String {
    let stage = if outcome.record.stage.is_empty() {
        "validate"
    } else {
        outcome.record.stage.as_str()
    };
    release_run_summary_text(
        stage,
        outcome.record.status.label(),
        outcome.record.waiting_on.as_deref(),
    )
}

/// The one-line end of a Run, naming what a paused Run waits on.
pub(super) fn release_run_summary_text(
    stage: &str,
    status: &str,
    waiting_on: Option<&str>,
) -> String {
    match waiting_on {
        Some(waiting) => {
            format!("release reached stage `{stage}` ({status}); waiting on `{waiting}`")
        }
        None => format!("release reached stage `{stage}` ({status})"),
    }
}

/// The release receipt for a Run id, or the active Run's receipt.
pub(super) fn release_receipt(
    resolved: &ResolvedTarget,
    run: Option<&str>,
) -> Result<effigy_release::ReleaseReceipt, RunnerError> {
    let home = release_home_via_release()?;
    Ok(release_receipt_for_run(
        &home,
        &resolved.resolved_root,
        run,
    )?)
}

pub(super) fn run_standalone_release_gates(
    resolved: &ResolvedTarget,
) -> Result<ReleaseGateRun, RunnerError> {
    let config = load_release_config(&resolved.resolved_root)?;
    // The inventory is printed before the first gate starts, so an operator can
    // tell a slow gate from a hung command. Each line names the gate's stage
    // and command, not only its name.
    emit_release_gate_inventory(&config.gates);
    if !config.gates.is_empty() {
        emit_release_progress_line("running standalone release gates");
    }
    let report = run_release_gates(&resolved.resolved_root, &config.gates, true);
    Ok(collect_release_gate_run(
        resolved.resolved_root.clone(),
        config.gates,
        report,
    ))
}

/// Print the configured gate inventory, one line per gate.
fn emit_release_gate_inventory(gates: &[ResolvedGate]) {
    if gates.is_empty() {
        emit_release_progress_line("no gates configured");
        return;
    }
    emit_release_progress_line(&format!("configured gates ({}):", gates.len()));
    for (index, gate) in gates.iter().enumerate() {
        emit_release_progress_line(&format!(
            "  {}/{} `{}` ({}) - {}",
            index + 1,
            gates.len(),
            gate.name,
            gate.stage.stage_name(),
            gate.command
        ));
    }
}

fn run_release_gates(root: &Path, gates: &[ResolvedGate], fail_fast: bool) -> GateExecutionReport {
    let mut observer = ReleaseGateSteps::default();
    run_release_gates_with_observer(root, gates, fail_fast, &mut observer)
}

/// Draw each gate as a live step: `running gate cargo test (2/5)` while it
/// runs, then one line saying whether it passed, how long it took and, when it
/// failed, that the log is where the detail is.
#[derive(Default)]
struct ReleaseGateSteps {
    running: Option<Step>,
}

impl ReleaseGateSteps {
    fn clear(&mut self) {
        if let Some(step) = self.running.take() {
            step.finish_clear();
        }
    }
}

impl Drop for ReleaseGateSteps {
    fn drop(&mut self) {
        self.clear();
    }
}

impl GateObserver for ReleaseGateSteps {
    fn gate_started(
        &mut self,
        position: usize,
        total: usize,
        gate: &ResolvedGate,
        reusing_hosted_evidence: bool,
    ) {
        self.clear();
        self.running = Some(live_step(gate_progress(
            position,
            total,
            gate,
            reusing_hosted_evidence,
        )));
    }

    fn gate_finished(&mut self, position: usize, total: usize, result: &GateResult) {
        let summary = release_gate_summary(position, total, result);
        match self.running.take() {
            Some(step) if result.passed => step.succeed(&summary),
            Some(step) => step.fail(&summary),
            None => outcome_line(result.passed, &summary),
        }
    }

    fn note(&mut self, message: &str) {
        self.clear();
        emit_release_progress_line(message);
    }
}

/// A gate's live step, for example `running gate cargo test (2/5); waiting for
/// `effigy test` to exit; 3s elapsed`. Reusing hosted evidence names the
/// evidence it waits on, because nothing local is running.
pub(super) fn gate_progress(
    position: usize,
    total: usize,
    gate: &ResolvedGate,
    reusing_hosted_evidence: bool,
) -> ProgressSource {
    let source = ProgressSource::waiting_for(String::new());
    show_gate(&source, position, total, gate, reusing_hosted_evidence);
    source
}

/// Show one gate on a progress source: its sub-step and count, and the exact
/// command or hosted evidence it is waiting on.
fn show_gate(
    source: &ProgressSource,
    position: usize,
    total: usize,
    gate: &ResolvedGate,
    reusing_hosted_evidence: bool,
) {
    let (sub_step, awaiting) = if reusing_hosted_evidence {
        (
            format!("reusing hosted evidence for gate {}", gate.name),
            format!("hosted evidence for gate {}", gate.name),
        )
    } else {
        (
            format!("running gate {}", gate.name),
            format!("`{}` to exit", gate.command),
        )
    };
    source.update(sub_step, Some((position as u64, total as u64)), None);
    source.waiting_on(awaiting);
}

/// The line a finished gate leaves: the outcome word, the timing, and where
/// the gate's own output went.
pub(super) fn release_gate_summary(position: usize, total: usize, result: &GateResult) -> String {
    let outcome = gate_outcome_word(result);
    let duration = effigy_kernel::human::human_duration(std::time::Duration::from_millis(
        result.duration_ms as u64,
    ));
    match result.log_path.as_deref() {
        Some(path) if !result.passed => format!(
            "gate `{}` {outcome} after {duration} ({position}/{total}); its output is in {}",
            result.name,
            path.display()
        ),
        _ => format!(
            "gate `{}` {outcome} after {duration} ({position}/{total})",
            result.name
        ),
    }
}

pub(super) fn emit_release_progress_line(message: &str) {
    eprintln!("[release] {message}");
}
