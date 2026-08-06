use serde::Serialize;

use crate::text::{remediation_hints_for_blockers, ReleaseBlockedStage};
use crate::{
    FileMutationPlan, GateResult, ReleaseExecutePlan, ReleaseExecuted, ReleaseGateRun,
    ReleasePreparePlan, ReleasePrepared, ReleaseSimulation, ReleaseStatus, ReleaseVerifyInstall,
    ResolvedVersionSource, VerificationStepResult,
};

pub fn render_release_status_json(status: &ReleaseStatus) -> String {
    render_pretty(
        &ReleaseStatusPayload::from(status),
        "{\"schema\":\"effigy.release.status.v1\",\"ready\":false}",
    )
}

pub fn render_release_gate_run_json(run: &ReleaseGateRun) -> String {
    render_pretty(
        &ReleaseGateRunPayload::from(run),
        "{\"schema\":\"effigy.release.gates.v1\",\"passed\":false}",
    )
}

pub fn render_release_verify_install_json(result: &ReleaseVerifyInstall) -> String {
    render_pretty(
        &ReleaseVerifyInstallPayload::from(result),
        "{\"schema\":\"effigy.release.verify-install.v1\",\"verified\":false}",
    )
}

pub fn render_release_prepare_plan_json(plan: &ReleasePreparePlan) -> String {
    render_pretty(
        &ReleasePreparePlanPayload::from(plan),
        "{\"schema\":\"effigy.release.prepare.plan.v1\",\"ready\":false}",
    )
}

pub fn render_release_simulation_json(simulation: &ReleaseSimulation) -> String {
    render_pretty(
        &ReleaseSimulationPayload::from(simulation),
        "{\"schema\":\"effigy.release.simulate.v1\",\"ready\":false}",
    )
}

pub fn render_release_prepared_json(result: &ReleasePrepared) -> String {
    render_pretty(
        &ReleasePreparedPayload::from(result),
        "{\"schema\":\"effigy.release.prepare.v1\",\"prepared\":false}",
    )
}

pub fn render_release_execute_plan_json(plan: &ReleaseExecutePlan) -> String {
    render_pretty(
        &ReleaseExecutePlanPayload::from(plan),
        "{\"schema\":\"effigy.release.execute.plan.v1\",\"ready\":false}",
    )
}

pub fn render_release_resume_json(
    plan: &ReleaseExecutePlan,
    suggested_actions: &[String],
) -> String {
    render_pretty(
        &ReleaseResumePayload::from_plan(plan, suggested_actions),
        "{\"schema\":\"effigy.release.resume.v1\",\"state_loaded\":false}",
    )
}

pub fn render_release_executed_json(result: &ReleaseExecuted) -> String {
    render_pretty(
        &ReleaseExecutedPayload::from(result),
        "{\"schema\":\"effigy.release.execute.v1\",\"executed\":false}",
    )
}

fn render_pretty<T: Serialize>(payload: &T, fallback: &str) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| fallback.to_owned())
}

#[derive(Serialize)]
struct VersionSourcePayload {
    file: String,
    format: &'static str,
    path: Option<String>,
}

impl From<&ResolvedVersionSource> for VersionSourcePayload {
    fn from(source: &ResolvedVersionSource) -> Self {
        Self {
            file: source.path.display().to_string(),
            format: source.kind.format_label(),
            path: source.field_path.clone(),
        }
    }
}

#[derive(Serialize)]
struct ChangelogPayload {
    path: String,
    valid: bool,
    diagnostic_count: usize,
    diagnostics: Vec<String>,
}

#[derive(Serialize)]
struct UnreleasedPayload {
    empty: bool,
    entry_count: usize,
    counts: std::collections::BTreeMap<String, usize>,
}

#[derive(Serialize)]
struct ReleaseGatesPayload {
    checked: bool,
    configured_count: usize,
    results: Vec<GateResultPayload>,
}

#[derive(Serialize)]
struct SimulationGatesPayload {
    configured_count: usize,
    executed_count: usize,
    stopped_early: bool,
    total_duration_ms: u128,
    results: Vec<GateResultPayload>,
}

#[derive(Serialize)]
struct ExecuteGatesPayload {
    checked: bool,
    passed: bool,
}

#[derive(Serialize)]
struct SourceFingerprintsPayload {
    available: bool,
    drift: Vec<String>,
}

#[derive(Serialize)]
struct WorkingTreePayload {
    expected_files: Vec<String>,
    modified_files: Vec<String>,
    missing_expected_files: Vec<String>,
    unexpected_files: Vec<String>,
}

#[derive(Serialize)]
struct DriftPayload {
    expected_files: Vec<String>,
    modified_files: Vec<String>,
    missing_expected_files: Vec<String>,
    unexpected_files: Vec<String>,
}

#[derive(Serialize)]
struct GateResultPayload {
    name: String,
    description: Option<String>,
    command: String,
    passed: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    launch_error: Option<String>,
    duration_ms: u128,
}

impl From<&GateResult> for GateResultPayload {
    fn from(gate: &GateResult) -> Self {
        Self {
            name: gate.name.clone(),
            description: gate.description.clone(),
            command: gate.command.clone(),
            passed: gate.passed,
            exit_code: gate.exit_code,
            stdout: gate.stdout.clone(),
            stderr: gate.stderr.clone(),
            launch_error: gate.launch_error.clone(),
            duration_ms: gate.duration_ms,
        }
    }
}

#[derive(Serialize)]
struct VerificationStepResultPayload {
    name: String,
    command: String,
    passed: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    launch_error: Option<String>,
    duration_ms: u128,
}

impl From<&VerificationStepResult> for VerificationStepResultPayload {
    fn from(step: &VerificationStepResult) -> Self {
        Self {
            name: step.name.clone(),
            command: step.command.clone(),
            passed: step.passed,
            exit_code: step.exit_code,
            stdout: step.stdout.clone(),
            stderr: step.stderr.clone(),
            launch_error: step.launch_error.clone(),
            duration_ms: step.duration_ms,
        }
    }
}

#[derive(Serialize)]
struct FileMutationPlanPayload {
    path: String,
    kind: &'static str,
    summary: String,
    before_preview: String,
    after_preview: String,
    detail_lines: Vec<String>,
    diff_preview: Vec<String>,
}

impl From<&FileMutationPlan> for FileMutationPlanPayload {
    fn from(mutation: &FileMutationPlan) -> Self {
        Self {
            path: mutation.path.display().to_string(),
            kind: mutation.kind,
            summary: mutation.summary.clone(),
            before_preview: mutation.before_preview.clone(),
            after_preview: mutation.after_preview.clone(),
            detail_lines: mutation.detail_lines.clone(),
            diff_preview: mutation.diff_preview.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseStatusPayload {
    schema: &'static str,
    schema_version: u8,
    ready: bool,
    repo_root: String,
    current_version: String,
    version_source: VersionSourcePayload,
    changelog: ChangelogPayload,
    unreleased: UnreleasedPayload,
    suggested_bump: String,
    next_version: Option<String>,
    tag: Option<String>,
    gates: ReleaseGatesPayload,
    blockers: Vec<String>,
}

impl From<&ReleaseStatus> for ReleaseStatusPayload {
    fn from(status: &ReleaseStatus) -> Self {
        Self {
            schema: "effigy.release.status.v1",
            schema_version: 1,
            ready: status.ready,
            repo_root: status.repo_root.display().to_string(),
            current_version: status.current_version.to_string(),
            version_source: VersionSourcePayload::from(&status.version_source),
            changelog: ChangelogPayload {
                path: status.changelog_path.display().to_string(),
                valid: status.changelog_valid,
                diagnostic_count: status.changelog_diagnostics.len(),
                diagnostics: status.changelog_diagnostics.clone(),
            },
            unreleased: UnreleasedPayload {
                empty: status.unreleased_empty,
                entry_count: status.unreleased_counts.values().copied().sum::<usize>(),
                counts: status.unreleased_counts.clone(),
            },
            suggested_bump: status.suggested_bump.clone(),
            next_version: status.next_version.as_ref().map(ToString::to_string),
            tag: status.tag.clone(),
            gates: ReleaseGatesPayload {
                checked: status.gates_checked,
                configured_count: status.configured_gate_count,
                results: status
                    .gate_results
                    .iter()
                    .map(GateResultPayload::from)
                    .collect(),
            },
            blockers: status.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseGateRunPayload {
    schema: &'static str,
    schema_version: u8,
    passed: bool,
    repo_root: String,
    configured_gate_count: usize,
    executed_gate_count: usize,
    stopped_early: bool,
    total_duration_ms: u128,
    results: Vec<GateResultPayload>,
    blockers: Vec<String>,
}

impl From<&ReleaseGateRun> for ReleaseGateRunPayload {
    fn from(run: &ReleaseGateRun) -> Self {
        Self {
            schema: "effigy.release.gates.v1",
            schema_version: 1,
            passed: run.passed,
            repo_root: run.repo_root.display().to_string(),
            configured_gate_count: run.configured_gate_count,
            executed_gate_count: run.executed_gate_count,
            stopped_early: run.stopped_early,
            total_duration_ms: run.total_duration_ms,
            results: run
                .gate_results
                .iter()
                .map(GateResultPayload::from)
                .collect(),
            blockers: run.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseVerifyInstallPayload {
    schema: &'static str,
    schema_version: u8,
    verified: bool,
    repo_root: String,
    tag: String,
    repo_url: String,
    installed_bin: Option<String>,
    configured_check_count: usize,
    executed_check_count: usize,
    stopped_early: bool,
    results: Vec<VerificationStepResultPayload>,
    blockers: Vec<String>,
}

impl From<&ReleaseVerifyInstall> for ReleaseVerifyInstallPayload {
    fn from(result: &ReleaseVerifyInstall) -> Self {
        Self {
            schema: "effigy.release.verify-install.v1",
            schema_version: 1,
            verified: result.verified,
            repo_root: result.repo_root.display().to_string(),
            tag: result.tag.clone(),
            repo_url: result.repo_url.clone(),
            installed_bin: result
                .installed_bin
                .as_ref()
                .map(|path| path.display().to_string()),
            configured_check_count: result.configured_check_count,
            executed_check_count: result.executed_check_count,
            stopped_early: result.stopped_early,
            results: result
                .results
                .iter()
                .map(VerificationStepResultPayload::from)
                .collect(),
            blockers: result.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleasePreparePlanPayload {
    schema: &'static str,
    schema_version: u8,
    mode: &'static str,
    ready: bool,
    repo_root: String,
    current_version: String,
    version_source: VersionSourcePayload,
    suggested_version: Option<String>,
    planned_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    release_date: String,
    gates: ReleaseGatesPayload,
    mutations: Vec<FileMutationPlanPayload>,
    blockers: Vec<String>,
}

impl From<&ReleasePreparePlan> for ReleasePreparePlanPayload {
    fn from(plan: &ReleasePreparePlan) -> Self {
        Self {
            schema: "effigy.release.prepare.plan.v1",
            schema_version: 1,
            mode: "plan",
            ready: plan.ready,
            repo_root: plan.repo_root.display().to_string(),
            current_version: plan.current_version.to_string(),
            version_source: VersionSourcePayload::from(&plan.version_source),
            suggested_version: plan.suggested_version.as_ref().map(ToString::to_string),
            planned_version: plan.planned_version.as_ref().map(ToString::to_string),
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date.clone(),
            gates: ReleaseGatesPayload {
                checked: plan.gates_checked,
                configured_count: plan.configured_gate_count,
                results: plan
                    .gate_results
                    .iter()
                    .map(GateResultPayload::from)
                    .collect(),
            },
            mutations: plan
                .mutations
                .iter()
                .map(FileMutationPlanPayload::from)
                .collect(),
            blockers: plan.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseSimulationPayload {
    schema: &'static str,
    schema_version: u8,
    mode: &'static str,
    ready: bool,
    repo_root: String,
    current_version: String,
    version_source: VersionSourcePayload,
    suggested_version: Option<String>,
    planned_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    release_date: String,
    commit_message: Option<String>,
    state_file: String,
    state_file_exists: bool,
    state_file_written: bool,
    gates: SimulationGatesPayload,
    mutations: Vec<FileMutationPlanPayload>,
    blockers: Vec<String>,
}

impl From<&ReleaseSimulation> for ReleaseSimulationPayload {
    fn from(simulation: &ReleaseSimulation) -> Self {
        Self {
            schema: "effigy.release.simulate.v1",
            schema_version: 1,
            mode: "simulate",
            ready: simulation.ready,
            repo_root: simulation.repo_root.display().to_string(),
            current_version: simulation.current_version.to_string(),
            version_source: VersionSourcePayload::from(&simulation.version_source),
            suggested_version: simulation
                .suggested_version
                .as_ref()
                .map(ToString::to_string),
            planned_version: simulation.planned_version.as_ref().map(ToString::to_string),
            suggested_tag: simulation.suggested_tag.clone(),
            tag: simulation.tag.clone(),
            version_override_used: simulation.version_override_used,
            release_date: simulation.release_date.clone(),
            commit_message: simulation.commit_message.clone(),
            state_file: simulation.state_file.display().to_string(),
            state_file_exists: simulation.state_file_exists,
            state_file_written: simulation.state_file_written,
            gates: SimulationGatesPayload {
                configured_count: simulation.configured_gate_count,
                executed_count: simulation.executed_gate_count,
                stopped_early: simulation.stopped_early,
                total_duration_ms: simulation.total_duration_ms,
                results: simulation
                    .gate_results
                    .iter()
                    .map(GateResultPayload::from)
                    .collect(),
            },
            mutations: simulation
                .mutations
                .iter()
                .map(FileMutationPlanPayload::from)
                .collect(),
            blockers: simulation.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleasePreparedPayload {
    schema: &'static str,
    schema_version: u8,
    prepared: bool,
    repo_root: String,
    previous_version: String,
    suggested_version: Option<String>,
    prepared_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    release_date: String,
    state_file: String,
    state_file_written: bool,
    gates: ReleaseGatesPayload,
    files_modified: Vec<String>,
    blockers: Vec<String>,
}

impl From<&ReleasePrepared> for ReleasePreparedPayload {
    fn from(result: &ReleasePrepared) -> Self {
        Self {
            schema: "effigy.release.prepare.v1",
            schema_version: 1,
            prepared: result.prepared,
            repo_root: result.repo_root.display().to_string(),
            previous_version: result.previous_version.to_string(),
            suggested_version: result.suggested_version.as_ref().map(ToString::to_string),
            prepared_version: result.prepared_version.as_ref().map(ToString::to_string),
            suggested_tag: result.suggested_tag.clone(),
            tag: result.tag.clone(),
            version_override_used: result.version_override_used,
            release_date: result.release_date.clone(),
            state_file: result.state_file.display().to_string(),
            state_file_written: result.state_file_written,
            gates: ReleaseGatesPayload {
                checked: result.gates_checked,
                configured_count: result.configured_gate_count,
                results: result
                    .gate_results
                    .iter()
                    .map(GateResultPayload::from)
                    .collect(),
            },
            files_modified: result
                .files_modified
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            blockers: result.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseExecutePlanPayload {
    schema: &'static str,
    schema_version: u8,
    mode: &'static str,
    ready: bool,
    repo_root: String,
    state_file: String,
    state_loaded: bool,
    previous_version: Option<String>,
    suggested_version: Option<String>,
    prepared_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    release_date: Option<String>,
    prepared_at: Option<String>,
    prepared_branch: Option<String>,
    prepared_head: Option<String>,
    stale: bool,
    stale_threshold_seconds: i64,
    stale_override_required: bool,
    stale_override_used: bool,
    branch: Option<String>,
    current_head: Option<String>,
    remote: Option<String>,
    gates: ExecuteGatesPayload,
    source_fingerprints: SourceFingerprintsPayload,
    working_tree: WorkingTreePayload,
    warnings: Vec<String>,
    blockers: Vec<String>,
    suggested_actions: Vec<String>,
}

impl From<&ReleaseExecutePlan> for ReleaseExecutePlanPayload {
    fn from(plan: &ReleaseExecutePlan) -> Self {
        Self {
            schema: "effigy.release.execute.plan.v1",
            schema_version: 1,
            mode: "plan",
            ready: plan.ready,
            repo_root: plan.repo_root.display().to_string(),
            state_file: plan.state_file.display().to_string(),
            state_loaded: plan.state_loaded,
            previous_version: plan.previous_version.as_ref().map(ToString::to_string),
            suggested_version: plan.suggested_version.as_ref().map(ToString::to_string),
            prepared_version: plan.prepared_version.as_ref().map(ToString::to_string),
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date.clone(),
            prepared_at: plan.prepared_at.clone(),
            prepared_branch: plan.prepared_branch.clone(),
            prepared_head: plan.prepared_head.clone(),
            stale: plan.stale,
            stale_threshold_seconds: plan.stale_threshold_seconds,
            stale_override_required: plan.stale_override_required,
            stale_override_used: plan.stale_override_used,
            branch: plan.branch.clone(),
            current_head: plan.current_head.clone(),
            remote: plan.remote.clone(),
            gates: ExecuteGatesPayload {
                checked: plan.gates_checked,
                passed: plan.gates_passed,
            },
            source_fingerprints: SourceFingerprintsPayload {
                available: plan.source_fingerprint_available,
                drift: plan.fingerprint_drift.clone(),
            },
            working_tree: WorkingTreePayload {
                expected_files: plan.expected_files.clone(),
                modified_files: plan.modified_files.clone(),
                missing_expected_files: plan.missing_expected_files.clone(),
                unexpected_files: plan.unexpected_files.clone(),
            },
            warnings: plan.warnings.clone(),
            blockers: plan.blockers.clone(),
            suggested_actions: remediation_hints_for_blockers(
                &plan.blockers,
                ReleaseBlockedStage::Execute,
            ),
        }
    }
}

#[derive(Serialize)]
struct ReleaseResumePayload {
    schema: &'static str,
    schema_version: u8,
    state_loaded: bool,
    review_available: bool,
    ready_to_execute: bool,
    repo_root: String,
    state_file: String,
    previous_version: Option<String>,
    suggested_version: Option<String>,
    prepared_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    release_date: Option<String>,
    prepared_at: Option<String>,
    prepared_branch: Option<String>,
    prepared_head: Option<String>,
    stale: bool,
    stale_override_required: bool,
    stale_override_used: bool,
    branch: Option<String>,
    current_head: Option<String>,
    remote: Option<String>,
    gates: ExecuteGatesPayload,
    source_fingerprints: SourceFingerprintsPayload,
    drift: DriftPayload,
    warnings: Vec<String>,
    blockers: Vec<String>,
    suggested_actions: Vec<String>,
}

impl ReleaseResumePayload {
    fn from_plan(plan: &ReleaseExecutePlan, suggested_actions: &[String]) -> Self {
        Self {
            schema: "effigy.release.resume.v1",
            schema_version: 1,
            state_loaded: plan.state_loaded,
            review_available: plan.state_loaded,
            ready_to_execute: plan.ready,
            repo_root: plan.repo_root.display().to_string(),
            state_file: plan.state_file.display().to_string(),
            previous_version: plan.previous_version.as_ref().map(ToString::to_string),
            suggested_version: plan.suggested_version.as_ref().map(ToString::to_string),
            prepared_version: plan.prepared_version.as_ref().map(ToString::to_string),
            suggested_tag: plan.suggested_tag.clone(),
            tag: plan.tag.clone(),
            version_override_used: plan.version_override_used,
            release_date: plan.release_date.clone(),
            prepared_at: plan.prepared_at.clone(),
            prepared_branch: plan.prepared_branch.clone(),
            prepared_head: plan.prepared_head.clone(),
            stale: plan.stale,
            stale_override_required: plan.stale_override_required,
            stale_override_used: plan.stale_override_used,
            branch: plan.branch.clone(),
            current_head: plan.current_head.clone(),
            remote: plan.remote.clone(),
            gates: ExecuteGatesPayload {
                checked: plan.gates_checked,
                passed: plan.gates_passed,
            },
            source_fingerprints: SourceFingerprintsPayload {
                available: plan.source_fingerprint_available,
                drift: plan.fingerprint_drift.clone(),
            },
            drift: DriftPayload {
                expected_files: plan.expected_files.clone(),
                modified_files: plan.modified_files.clone(),
                missing_expected_files: plan.missing_expected_files.clone(),
                unexpected_files: plan.unexpected_files.clone(),
            },
            warnings: plan.warnings.clone(),
            blockers: plan.blockers.clone(),
            suggested_actions: suggested_actions.to_vec(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseExecutedPayload {
    schema: &'static str,
    schema_version: u8,
    executed: bool,
    repo_root: String,
    state_file: String,
    previous_version: Option<String>,
    suggested_version: Option<String>,
    prepared_version: Option<String>,
    suggested_tag: Option<String>,
    tag: Option<String>,
    version_override_used: bool,
    branch: Option<String>,
    remote: Option<String>,
    release_date: Option<String>,
    prepared_at: Option<String>,
    prepared_branch: Option<String>,
    prepared_head: Option<String>,
    commit_message: Option<String>,
    commit_sha: Option<String>,
    current_head: Option<String>,
    stale: bool,
    stale_override_used: bool,
    fingerprint_drift: Vec<String>,
    committed: bool,
    tag_created: bool,
    pushed: bool,
    state_file_removed: bool,
    files_committed: Vec<String>,
    warnings: Vec<String>,
    blockers: Vec<String>,
    post_release_instructions: Vec<String>,
}

impl From<&ReleaseExecuted> for ReleaseExecutedPayload {
    fn from(result: &ReleaseExecuted) -> Self {
        Self {
            schema: "effigy.release.execute.v1",
            schema_version: 1,
            executed: result.executed,
            repo_root: result.repo_root.display().to_string(),
            state_file: result.state_file.display().to_string(),
            previous_version: result.previous_version.as_ref().map(ToString::to_string),
            suggested_version: result.suggested_version.as_ref().map(ToString::to_string),
            prepared_version: result.prepared_version.as_ref().map(ToString::to_string),
            suggested_tag: result.suggested_tag.clone(),
            tag: result.tag.clone(),
            version_override_used: result.version_override_used,
            branch: result.branch.clone(),
            remote: result.remote.clone(),
            release_date: result.release_date.clone(),
            prepared_at: result.prepared_at.clone(),
            prepared_branch: result.prepared_branch.clone(),
            prepared_head: result.prepared_head.clone(),
            commit_message: result.commit_message.clone(),
            commit_sha: result.commit_sha.clone(),
            current_head: result.current_head.clone(),
            stale: result.stale,
            stale_override_used: result.stale_override_used,
            fingerprint_drift: result.fingerprint_drift.clone(),
            committed: result.committed,
            tag_created: result.tag_created,
            pushed: result.pushed,
            state_file_removed: result.state_file_removed,
            files_committed: result.files_committed.clone(),
            warnings: result.warnings.clone(),
            blockers: result.blockers.clone(),
            post_release_instructions: result.post_release_instructions.clone(),
        }
    }
}
