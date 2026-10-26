use serde::Serialize;

use crate::{
    GateResult, ReleaseGateRun, ReleasePipelinePlan, ReleaseStatus, ResolvedTarget,
    ResolvedVersionSource, TargetKind,
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

pub fn render_release_plan_json(plan: &ReleasePipelinePlan) -> String {
    render_pretty(
        &ReleasePipelinePlanPayload::from(plan),
        "{\"schema\":\"effigy.release.plan.v1\",\"ready\":false}",
    )
}

fn render_pretty<T: Serialize>(payload: &T, fallback: &str) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| fallback.to_owned())
}

fn optional_path(path: &Option<std::path::PathBuf>) -> Option<String> {
    path.as_ref().map(|value| value.display().to_string())
}

fn is_false(value: &bool) -> bool {
    !*value
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
    /// False for a release with no `semver` identity: it never reads the
    /// changelog, so `valid` is vacuous there.
    used: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    log_path: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    reused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    hosted_run_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hosted_head_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hosted_repository: Option<String>,
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
            log_path: optional_path(&gate.log_path),
            reused: gate.reused,
            hosted_run_url: gate.hosted_run_url.clone(),
            hosted_head_sha: gate.hosted_head_sha.clone(),
            hosted_repository: gate.hosted_repository.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseStatusPayload {
    schema: &'static str,
    schema_version: u8,
    ready: bool,
    gates_passed: bool,
    gate_check_passed: bool,
    repo_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_source: Option<VersionSourcePayload>,
    changelog: ChangelogPayload,
    unreleased: UnreleasedPayload,
    suggested_bump: String,
    next_version: Option<String>,
    tag: Option<String>,
    gates: ReleaseGatesPayload,
    config_shape: &'static str,
    identities: Vec<ReleaseIdentityPayload>,
    plan: ReleasePlanSummaryPayload,
    run: ReleaseRunPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment_path: Option<String>,
    blockers: Vec<String>,
}

impl From<&ReleaseStatus> for ReleaseStatusPayload {
    fn from(status: &ReleaseStatus) -> Self {
        Self {
            schema: "effigy.release.status.v1",
            schema_version: 1,
            ready: status.ready,
            gates_passed: status.gates_passed,
            gate_check_passed: status.gate_check_passed(),
            repo_root: status.repo_root.display().to_string(),
            current_version: status.current_version.as_ref().map(ToString::to_string),
            version_source: status
                .version_source
                .as_ref()
                .map(VersionSourcePayload::from),
            changelog: ChangelogPayload {
                path: status.changelog_path.display().to_string(),
                used: status.plan.changelog_in_use,
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
            environment_path: optional_path(&status.environment_path),
            config_shape: if status.plan.single_identity_shorthand {
                "single-identity-shorthand"
            } else {
                "named-identities"
            },
            identities: status
                .plan
                .identities
                .iter()
                .map(ReleaseIdentityPayload::from)
                .collect(),
            plan: ReleasePlanSummaryPayload::from(&status.plan),
            run: ReleaseRunPayload::from_summary(&status.run),
            blockers: status.blockers.clone(),
        }
    }
}

/// The parts of the plan `release status` reports alongside its readiness:
/// what would change, where it would go, and where the Run would stop.
#[derive(Serialize)]
struct ReleasePlanSummaryPayload {
    files: Vec<String>,
    gates: Vec<PlanGatePayload>,
    artifacts: Vec<ReleaseArtifactPayload>,
    targets: Vec<ReleaseTargetPayload>,
    approvals: ReleaseApprovalsPayload,
    working_tree_clean: Option<bool>,
}

impl From<&ReleasePipelinePlan> for ReleasePlanSummaryPayload {
    fn from(plan: &ReleasePipelinePlan) -> Self {
        Self {
            files: plan
                .files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            gates: plan.gates.iter().map(PlanGatePayload::from).collect(),
            artifacts: plan
                .artifacts
                .iter()
                .map(|artifact| ReleaseArtifactPayload {
                    name: artifact.name.clone(),
                    task: artifact.task.clone(),
                    path: artifact.path.clone(),
                    identity_kind: artifact.identity_kind.clone(),
                })
                .collect(),
            targets: plan
                .targets
                .iter()
                .map(ReleaseTargetPayload::from)
                .collect(),
            approvals: ReleaseApprovalsPayload {
                checkpoints: plan.approvals.clone(),
                defaulted: plan.approval_defaulted,
            },
            working_tree_clean: plan.working_tree_clean,
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
    gates: Vec<PlanGatePayload>,
    results: Vec<GateResultPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    environment_path: Option<String>,
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
            // The configured inventory the progress lines streamed, so a JSON
            // reader sees the same gates, stages and commands.
            gates: run.gates.iter().map(PlanGatePayload::from).collect(),
            results: run
                .gate_results
                .iter()
                .map(GateResultPayload::from)
                .collect(),
            environment_path: optional_path(&run.environment_path),
            blockers: run.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleasePipelinePlanPayload {
    schema: &'static str,
    schema_version: u8,
    mode: &'static str,
    ready: bool,
    repo_root: String,
    config_shape: &'static str,
    changelog: ChangelogPayload,
    unreleased: UnreleasedPayload,
    identities: Vec<ReleaseIdentityPayload>,
    files: Vec<String>,
    gates: Vec<PlanGatePayload>,
    artifacts: Vec<ReleaseArtifactPayload>,
    targets: Vec<ReleaseTargetPayload>,
    approvals: ReleaseApprovalsPayload,
    working_tree_clean: Option<bool>,
    run: ReleaseRunPayload,
    blockers: Vec<String>,
}

impl From<&ReleasePipelinePlan> for ReleasePipelinePlanPayload {
    fn from(plan: &ReleasePipelinePlan) -> Self {
        Self {
            schema: "effigy.release.plan.v1",
            schema_version: 1,
            mode: "plan",
            ready: plan.ready,
            repo_root: plan.repo_root.display().to_string(),
            config_shape: if plan.single_identity_shorthand {
                "single-identity-shorthand"
            } else {
                "named-identities"
            },
            changelog: ChangelogPayload {
                path: plan.changelog_path.display().to_string(),
                used: plan.changelog_in_use,
                valid: plan.changelog_valid,
                diagnostic_count: plan.changelog_diagnostics.len(),
                diagnostics: plan.changelog_diagnostics.clone(),
            },
            unreleased: UnreleasedPayload {
                empty: plan.unreleased_empty,
                entry_count: plan.unreleased_counts.values().copied().sum::<usize>(),
                counts: plan.unreleased_counts.clone(),
            },
            identities: plan
                .identities
                .iter()
                .map(ReleaseIdentityPayload::from)
                .collect(),
            files: plan
                .files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            gates: plan.gates.iter().map(PlanGatePayload::from).collect(),
            artifacts: plan
                .artifacts
                .iter()
                .map(|artifact| ReleaseArtifactPayload {
                    name: artifact.name.clone(),
                    task: artifact.task.clone(),
                    path: artifact.path.clone(),
                    identity_kind: artifact.identity_kind.clone(),
                })
                .collect(),
            targets: plan
                .targets
                .iter()
                .map(ReleaseTargetPayload::from)
                .collect(),
            approvals: ReleaseApprovalsPayload {
                checkpoints: plan.approvals.clone(),
                defaulted: plan.approval_defaulted,
            },
            working_tree_clean: plan.working_tree_clean,
            run: ReleaseRunPayload::none(),
            blockers: plan.blockers.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseIdentityPayload {
    name: String,
    kind: &'static str,
    current_version: Option<String>,
    next_version: Option<String>,
    identity: Option<String>,
    bump: Option<String>,
    tag: Option<String>,
    follows: Option<String>,
    version_file: Option<String>,
    task: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    files: Vec<String>,
}

impl From<&crate::ReleaseIdentityPlan> for ReleaseIdentityPayload {
    fn from(identity: &crate::ReleaseIdentityPlan) -> Self {
        Self {
            name: identity.name.clone(),
            kind: identity.kind.label(),
            current_version: identity.current_version.as_ref().map(ToString::to_string),
            next_version: identity.next_version.as_ref().map(ToString::to_string),
            identity: identity.identity_value.clone(),
            bump: identity.bump.clone(),
            tag: identity.tag.clone(),
            follows: identity.follows.clone(),
            version_file: identity
                .version_source
                .as_ref()
                .map(|source| source.path.display().to_string()),
            task: identity.task.clone(),
            files: identity
                .files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
        }
    }
}

#[derive(Serialize)]
struct PlanGatePayload {
    name: String,
    stage: &'static str,
    command: String,
    task: Option<String>,
    description: Option<String>,
    reuse_hosted_evidence: bool,
}

impl From<&crate::ResolvedGate> for PlanGatePayload {
    fn from(gate: &crate::ResolvedGate) -> Self {
        Self {
            name: gate.name.clone(),
            stage: gate.stage.stage_name(),
            command: gate.command.clone(),
            task: gate.task.clone(),
            description: gate.description.clone(),
            reuse_hosted_evidence: gate.reuse_hosted_evidence,
        }
    }
}

#[derive(Serialize)]
struct ReleaseArtifactPayload {
    name: String,
    task: String,
    path: String,
    identity_kind: Option<String>,
}

#[derive(Serialize)]
struct ReleaseTargetPayload {
    name: String,
    kind: &'static str,
    task: Option<String>,
    check: Option<String>,
    verify: Option<String>,
}

impl From<&ResolvedTarget> for ReleaseTargetPayload {
    fn from(target: &ResolvedTarget) -> Self {
        Self {
            name: target.name.clone(),
            kind: match target.kind {
                TargetKind::Tag => "tag",
                TargetKind::Task => "task",
            },
            task: target.task.clone(),
            check: target.check.clone(),
            verify: target.verify.clone(),
        }
    }
}

#[derive(Serialize)]
struct ReleaseApprovalsPayload {
    checkpoints: Vec<String>,
    defaulted: bool,
}

#[derive(Serialize)]
struct ReleaseRunPayload {
    active: bool,
    id: Option<String>,
    stage: Option<String>,
    waiting_on: Option<String>,
    status: Option<String>,
}

impl ReleaseRunPayload {
    fn none() -> Self {
        Self {
            active: false,
            id: None,
            stage: None,
            waiting_on: None,
            status: None,
        }
    }

    fn from_summary(run: &crate::ReleaseRunSummary) -> Self {
        Self {
            active: run.is_active(),
            id: run.id.clone(),
            stage: run.stage.clone(),
            waiting_on: run.waiting_on.clone(),
            status: run.status.clone(),
        }
    }
}
