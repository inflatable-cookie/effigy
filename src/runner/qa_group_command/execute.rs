//! QA-group run coordinator: one run identity, sequential member execution
//! through the canonical pipeline, one durable ledger.
//!
//! Members run in declaration order, one at a time, each as a typed
//! `TaskExecutionRequest` handed to the ordinary pipeline (routing,
//! environment, secrets, containers, locks, and cleanup stay pipeline
//! responsibilities). A group with a heavy member is admitted once, before
//! its first member; members run inside that admission and never queue
//! again (Q-006). A failed
//! member ends the group; later members are recorded `not_started` and no
//! pass is synthesized. Expected wall time is compared with execution wall
//! time only; over-budget evidence never changes the check outcome.

use std::path::Path;
use std::time::Instant;

use chrono::Utc;
use effigy_execution::{
    ExecutionEnvironmentPlan, QaGroupBackend, QaGroupBudgetState, QaGroupGapSnapshot,
    QaGroupMemberRecord, QaGroupMemberState, QaGroupOutcome, QaGroupRunGroupSnapshot,
    QaGroupRunHead, QaGroupRunRecord, QaGroupRunState, QaGroupRunTiming, QA_GROUP_RUN_SCHEMA,
};
use effigy_tasks::{budget_state, QaGroupPlan, QaGroupPlanMember};

use super::super::error::RunnerError;
use super::super::execute::api::run_manifest_task_request;
use super::member_runtime_context;
use effigy_runtime::qa_group_status::{
    begin_qa_group_run_record, finalize_qa_group_run_record, update_qa_group_run_record,
    write_qa_group_member_log,
};

/// Captured output from one member attempt. The typed pipeline error travels
/// unchanged so cancelled versus failed versus blocked stay distinguishable;
/// the error is boxed to keep the `Err` variant small.
type MemberAttempt = Result<(String, String), (Box<RunnerError>, String, String)>;

pub(super) fn execute_group_run(
    root: &Path,
    plan: &QaGroupPlan,
    output_json: bool,
) -> Result<String, RunnerError> {
    let selector = format!("qa-group:{}/{}", plan.group.catalog, plan.group.name);
    // The group run is a kernel run: its id is the run id, its liveness the
    // run's gate, its outcome the run's row. A group with a heavy member is
    // admitted once, here; its members run inside that admission (Q-006).
    let admission = plan
        .admission
        .required
        .then_some(effigy_manifest::ManifestTaskAdmission::Heavy);
    let run = crate::runner::task_engine::TaskRun::standalone(root, &selector, admission)?;
    let run_id = run.id().to_string();
    let mut record = initial_record(&run_id, plan);
    record.backend = run.admission_wait_ms().map(|waited| QaGroupBackend {
        kind: "admission_hook".to_owned(),
        queue_wait_ms: Some(waited),
    });
    record.timing.admission_wait_ms = run.admission_wait_ms();

    // Record writes are observability: a failed write warns and never
    // changes the group's outcome (K10).
    if let Err(error) = begin_qa_group_run_record(root, &record) {
        eprintln!("warning: qa-group run {run_id}: {error}");
    }

    // Retained group evidence includes member history and logs. Enter the
    // durable parent before classification so local members cannot downgrade.
    let scope = run.enter();
    let outcome = run_members(root, plan, &mut record);
    drop(scope);
    if let Err(error) = finalize_qa_group_run_record(root, &record) {
        eprintln!("warning: qa-group run {run_id}: {error}");
    }
    // The run's durable closeout carries the outcome the ledger recorded: a
    // cancelled group closes as cancelled, never as a failed check, so
    // `run status` and the group record agree.
    let workload = match &outcome {
        Ok(()) => effigy_kernel::outcome::WorkloadOutcome::Succeeded,
        Err(_) if record.outcome == Some(QaGroupOutcome::Cancelled) => {
            effigy_kernel::outcome::WorkloadOutcome::Cancelled {
                cause: effigy_kernel::ctx::StopCause::Requested,
                signal: None,
            }
        }
        Err(_) => effigy_kernel::outcome::WorkloadOutcome::Failed { code: 1 },
    };
    for warning in run.close(workload) {
        eprintln!("warning: qa-group run {run_id}: {warning}");
    }
    match outcome {
        Ok(()) => {
            if output_json {
                render_record_json(&record)
            } else {
                Ok(render_record_text(&record))
            }
        }
        Err(failure) => {
            let human = failure.human_error();
            let outcome = Some(failure.workload_outcome());
            Err(if output_json {
                RunnerError::structured_json(
                    serde_json::to_value(&record)
                        .map_err(|e| RunnerError::task_invocation(e.to_string()))?,
                    true,
                    human,
                    outcome,
                )
            } else {
                RunnerError::structured_text(render_record_text(&record), human, outcome)
            })
        }
    }
}

/// Run members serially and finalize aggregate states.
fn run_members(
    root: &Path,
    plan: &QaGroupPlan,
    record: &mut QaGroupRunRecord,
) -> Result<(), RunnerError> {
    record.timing.queued_at = Some(Utc::now().to_rfc3339());

    record.state = QaGroupRunState::Running;
    record.timing.started_at = Some(Utc::now().to_rfc3339());
    touch(record);
    let _ = update_qa_group_run_record(root, record);

    let started = Instant::now();
    let result = execute_member_loop(root, plan, record);
    record.timing.execution_wall_ms = Some(elapsed_ms(started));
    record.timing.ended_at = Some(Utc::now().to_rfc3339());
    record.budget_state = match budget_state(
        record.timing.expected_wall_ms,
        record.timing.execution_wall_ms,
    ) {
        "over_budget" => QaGroupBudgetState::OverBudget,
        "within_budget" => QaGroupBudgetState::WithinBudget,
        _ => QaGroupBudgetState::Unknown,
    };
    record.outcome = Some(match &result {
        // The loop already stamped a failure or cancellation outcome.
        Err(_) => record.outcome.unwrap_or(QaGroupOutcome::Failed),
        Ok(()) => QaGroupOutcome::Passed,
    });
    record.state = QaGroupRunState::Completed;
    touch(record);

    result
}

fn execute_member_loop(
    root: &Path,
    plan: &QaGroupPlan,
    record: &mut QaGroupRunRecord,
) -> Result<(), RunnerError> {
    for (index, member) in plan.members.iter().enumerate() {
        let started_at = Utc::now().to_rfc3339();
        {
            let entry =
                member_record_mut(record, &member.id).expect("plan members seed the ledger");
            entry.started_at = Some(started_at.clone());
        }
        touch(record);
        let _ = update_qa_group_run_record(root, record);

        let member_started = Instant::now();
        let attempt: MemberAttempt = run_single_member(root, member);
        let wall = elapsed_ms(member_started);
        let ended_at = Utc::now().to_rfc3339();
        let log_ref = member_log_ref(&record.run_id, &member.id);

        match &attempt {
            Ok((stdout, stderr)) => {
                write_member_log(
                    root,
                    &record.run_id,
                    member,
                    &started_at,
                    "exit 0",
                    stdout,
                    stderr,
                );
                let entry = member_record_mut(record, &member.id).expect("seeded");
                entry.state = QaGroupMemberState::Passed;
                entry.ended_at = Some(ended_at);
                entry.wall_ms = Some(wall);
                entry.exit_code = Some(0);
                entry.summary = Some("completed successfully".to_owned());
                entry.log_ref = Some(log_ref);
            }
            Err((failure, stdout, stderr)) => {
                let (state, summary, exit_code) = classify_member_failure(failure);
                write_member_log(
                    root,
                    &record.run_id,
                    member,
                    &started_at,
                    &format!("{}: {summary}", state.as_str()),
                    stdout,
                    stderr,
                );
                {
                    let entry = member_record_mut(record, &member.id).expect("seeded");
                    entry.state = state;
                    entry.ended_at = Some(ended_at);
                    entry.wall_ms = Some(wall);
                    entry.exit_code = exit_code;
                    entry.summary = Some(summary.clone());
                    entry.log_ref = Some(log_ref);
                }
                record.outcome = Some(classify_group_failure(failure));
                let reason = format!(
                    "member `{}` {} ({}); later members did not start",
                    member.id,
                    state.as_str(),
                    summary
                );
                for later in record.members[index + 1..]
                    .iter_mut()
                    .filter(|entry| entry.id != member.id)
                {
                    later.state = QaGroupMemberState::NotStarted;
                    later.not_started_reason = Some(reason.clone());
                }
            }
        }
        touch(record);
        let _ = update_qa_group_run_record(root, record);

        match attempt {
            Ok(_) => {}
            Err((failure, _, _)) => return Err(*failure),
        }
    }
    Ok(())
}

/// Run one member through the canonical pipeline.
///
/// The member's fixed argv is the declared args; `--json` is appended purely
/// as the capture vehicle (the pipeline strips it before the task command)
/// so run-scoped logs hold the pipeline's redacted captures.
fn run_single_member(root: &Path, member: &QaGroupPlanMember) -> MemberAttempt {
    let request = match build_member_request(root, member) {
        Ok(request) => request,
        Err(error) => return Err((Box::new(error), String::new(), String::new())),
    };

    match run_manifest_task_request(request) {
        Ok(rendered) => {
            let (stdout, stderr) = extract_payload_output(&rendered);
            Ok((stdout, stderr))
        }
        Err(failure @ RunnerError::TaskJsonFailure(_)) => {
            let (stdout, stderr) = task_failure_output(&failure);
            Err((Box::new(failure), stdout, stderr))
        }
        Err(RunnerError::CommandJsonFailure(failure)) => {
            // The captured payload carries the task's real exit code and its
            // redacted output. Re-derive the typed failure from them so the
            // ledger can distinguish cancellation (exit 130) from failure
            // and record the actual code; the group renders its own ledger,
            // so the wrapped JSON payload is not needed downstream.
            let (stdout, stderr, exit_code) = extract_failure_payload(failure.value.as_ref());
            Err((
                Box::new(RunnerError::TaskCommandFailure {
                    command: member_command_label(member),
                    code: Some(exit_code),
                    stdout: stdout.clone(),
                    stderr: stderr.clone(),
                }),
                stdout,
                stderr,
            ))
        }
        Err(failure) => {
            let (stdout, stderr) = task_failure_output(&failure);
            Err((Box::new(failure), stdout, stderr))
        }
    }
}

fn member_command_label(member: &QaGroupPlanMember) -> String {
    format!(
        "qa-group member {}/{} {:?}",
        member.catalog, member.task, member.args
    )
}

fn build_member_request(
    root: &Path,
    member: &QaGroupPlanMember,
) -> Result<effigy_execution::TaskExecutionRequest, RunnerError> {
    let mut args = member.args.clone();
    args.push("--json".to_owned());
    effigy_execution::TaskExecutionRequestBuilder::new()
        .runtime_context(member_runtime_context(root)?)
        .task(format!("{}/{}", member.catalog, member.task), args)
        .surface(if member.surface == "draft" {
            effigy_execution::ExecutionSurface::Draft
        } else {
            effigy_execution::ExecutionSurface::QaGroup
        })
        .environment(ExecutionEnvironmentPlan::default().cwd(root.to_path_buf()))
        .build()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn member_log_ref(run_id: &str, member_id: &str) -> String {
    format!(
        ".effigy/reports/qa-groups/{run_id}/members/{}",
        effigy_execution::member_log_file_name(member_id)
    )
}

fn write_member_log(
    root: &Path,
    run_id: &str,
    member: &QaGroupPlanMember,
    started_at: &str,
    exit_note: &str,
    stdout: &str,
    stderr: &str,
) {
    let mut content = String::new();
    content.push_str(&format!(
        "member: {}\nsurface: {}\nselector: {}/{}\nargs: {:?}\nstarted: {started_at}\n{exit_note}\n",
        member.id, member.surface, member.catalog, member.task, member.args
    ));
    if !stdout.trim().is_empty() {
        content.push_str("\n--- stdout (pipeline-redacted capture) ---\n");
        content.push_str(stdout);
        if !stdout.ends_with('\n') {
            content.push('\n');
        }
    }
    if !stderr.trim().is_empty() {
        content.push_str("\n--- stderr (pipeline-redacted capture) ---\n");
        content.push_str(stderr);
        if !stderr.ends_with('\n') {
            content.push('\n');
        }
    }
    if let Err(error) = write_qa_group_member_log(root, run_id, &member.id, &content) {
        eprintln!(
            "warning: could not persist member log for `{}`: {error}",
            member.id
        );
    }
}

/// Extract stdout/stderr from a successful captured task-run payload.
fn extract_payload_output(rendered: &str) -> (String, String) {
    match serde_json::from_str::<serde_json::Value>(rendered) {
        Ok(value) => (
            string_field(&value, "stdout"),
            string_field(&value, "stderr"),
        ),
        Err(_) => (String::new(), String::new()),
    }
}

fn extract_failure_payload(value: Option<&serde_json::Value>) -> (String, String, i32) {
    match value {
        Some(value) => (
            string_field(value, "stdout"),
            string_field(value, "stderr"),
            value
                .get("exit_code")
                .and_then(|code| code.as_i64())
                .and_then(|code| i32::try_from(code).ok())
                .unwrap_or(1),
        ),
        None => (String::new(), String::new(), 1),
    }
}

fn string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn task_failure_output(error: &RunnerError) -> (String, String) {
    match error {
        RunnerError::TaskCommandFailure { stdout, stderr, .. } => (stdout.clone(), stderr.clone()),
        RunnerError::TaskJsonFailure(failure) => (failure.stdout.clone(), failure.stderr.clone()),
        _ => (String::new(), String::new()),
    }
}

fn classify_member_failure(error: &RunnerError) -> (QaGroupMemberState, String, Option<i32>) {
    match error {
        RunnerError::TaskJsonFailure(failure)
            if matches!(
                failure.outcome,
                effigy_kernel::outcome::WorkloadOutcome::Cancelled { .. }
            ) =>
        {
            (
                QaGroupMemberState::Cancelled,
                format!("interrupted (exit {})", failure.code),
                Some(failure.code),
            )
        }
        RunnerError::TaskJsonFailure(failure) => (
            QaGroupMemberState::Failed,
            format!("task command failed with exit {}", failure.code),
            Some(failure.code),
        ),
        RunnerError::TaskCommandFailure { code, .. } if *code == Some(130) => (
            QaGroupMemberState::Cancelled,
            "interrupted (exit 130)".to_owned(),
            Some(130),
        ),
        RunnerError::TaskCommandFailure { code, .. } => (
            QaGroupMemberState::Failed,
            format!("task command failed with exit {}", code.unwrap_or(1)),
            Some(code.unwrap_or(1)),
        ),
        RunnerError::CommandJsonFailure(_) => (
            QaGroupMemberState::Failed,
            "task command failed (captured payload)".to_owned(),
            Some(1),
        ),
        RunnerError::TaskLockConflict(_) => (
            QaGroupMemberState::Blocked,
            "blocked by an active task lock".to_owned(),
            None,
        ),
        RunnerError::TaskCommandLaunch { .. } => (
            QaGroupMemberState::Blocked,
            "failed to launch the task command".to_owned(),
            None,
        ),
        other => (
            QaGroupMemberState::Failed,
            format!("runner error: {}", short_message(other)),
            None,
        ),
    }
}

fn classify_group_failure(error: &RunnerError) -> QaGroupOutcome {
    match error {
        RunnerError::TaskCommandFailure { code, .. } if *code == Some(130) => {
            QaGroupOutcome::Cancelled
        }
        RunnerError::TaskJsonFailure(failure)
            if matches!(
                failure.outcome,
                effigy_kernel::outcome::WorkloadOutcome::Cancelled { .. }
            ) =>
        {
            QaGroupOutcome::Cancelled
        }
        _ => QaGroupOutcome::Failed,
    }
}

fn short_message(error: &RunnerError) -> String {
    error
        .to_string()
        .lines()
        .next()
        .unwrap_or("runner error")
        .to_owned()
}

fn initial_record(run_id: &str, plan: &QaGroupPlan) -> QaGroupRunRecord {
    // unchanged shape
    QaGroupRunRecord {
        schema: QA_GROUP_RUN_SCHEMA.to_owned(),
        schema_version: 1,
        run_id: run_id.to_owned(),
        group: QaGroupRunGroupSnapshot {
            surface: plan.group.surface.clone(),
            name: plan.group.name.clone(),
            catalog: plan.group.catalog.clone(),
            source: plan.group.source.clone(),
            definition_sha256: plan.group.definition_sha256.clone(),
            scope_policy: plan.group.scope_policy.clone(),
            coverage_gaps: plan
                .group
                .coverage_gaps
                .iter()
                .map(|gap| QaGroupGapSnapshot {
                    input: gap.input.clone(),
                    reason: gap.reason.clone(),
                })
                .collect(),
            lifecycle: plan.group.lifecycle.clone(),
            expired: plan.group.expired,
        },
        head: QaGroupRunHead {
            commit: plan.head.commit.clone(),
            worktree: plan.head.worktree.clone(),
        },
        selected_targets: plan
            .members
            .iter()
            .flat_map(|member| member.targets.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect(),
        scope_inputs: plan.scope_inputs.clone(),
        scope_assessment: match plan.scope_assessment {
            effigy_tasks::ScopeAssessment::DeclaredMatch => "declared_match".to_owned(),
            effigy_tasks::ScopeAssessment::NotRequested => "not_requested".to_owned(),
            effigy_tasks::ScopeAssessment::NeedsPlanner => "needs_planner".to_owned(),
        },
        scope_matches: plan
            .scope_matches
            .iter()
            .map(|match_entry| effigy_execution::QaGroupScopeMatchRecord {
                input: match_entry.input.clone(),
                member_ids: match_entry.member_ids.clone(),
                gap_reasons: match_entry.gap_reasons.clone(),
            })
            .collect(),
        state: QaGroupRunState::Running,
        outcome: None,
        budget_state: QaGroupBudgetState::Unknown,
        timing: QaGroupRunTiming {
            expected_wall_ms: plan.group.expected_wall_ms,
            ..QaGroupRunTiming::default()
        },
        owner_pid: std::process::id(),
        owner_start_identity: effigy_process::process_start_identity(std::process::id()),
        boot_identity: effigy_process::boot_identity(),
        updated_at: Some(Utc::now().to_rfc3339()),
        members: plan
            .members
            .iter()
            .map(|member| QaGroupMemberRecord {
                id: member.id.clone(),
                kind: member.kind.clone(),
                surface: member.surface.clone(),
                catalog: member.catalog.clone(),
                selector: format!("{}/{}", member.catalog, member.task),
                args: member.args.clone(),
                targets: member.targets.clone(),
                covers: member.covers.clone(),
                state: QaGroupMemberState::NotStarted,
                started_at: None,
                ended_at: None,
                wall_ms: None,
                exit_code: None,
                log_ref: None,
                not_started_reason: None,
                summary: None,
            })
            .collect(),
        log_dir: format!(".effigy/reports/qa-groups/{run_id}/members"),
        warnings: Vec::new(),
        backend: None,
    }
}

fn member_record_mut<'a>(
    record: &'a mut QaGroupRunRecord,
    id: &str,
) -> Option<&'a mut QaGroupMemberRecord> {
    record.members.iter_mut().find(|entry| entry.id == id)
}

fn touch(record: &mut QaGroupRunRecord) {
    record.updated_at = Some(Utc::now().to_rfc3339());
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn render_record_json(record: &QaGroupRunRecord) -> Result<String, RunnerError> {
    let value = serde_json::to_value(record).map_err(|error| {
        RunnerError::task_invocation(format!("failed to encode run record: {error}"))
    })?;
    effigy_ui::encode_json(&value, true)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_record_text(record: &QaGroupRunRecord) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "QA group run {} for {}/{}: {} ({})\n",
        record.run_id,
        record.group.catalog,
        record.group.name,
        record
            .outcome
            .map(|outcome| outcome.as_str())
            .unwrap_or("incomplete"),
        record.state.as_str()
    ));
    for member in &record.members {
        let wall = member
            .wall_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "unknown".to_owned());
        out.push_str(&format!(
            "- {} [{}]{}{}\n",
            member.id,
            member.state.as_str(),
            member
                .summary
                .as_deref()
                .map(|summary| format!(" ({summary})"))
                .unwrap_or_default(),
            member
                .not_started_reason
                .as_deref()
                .map(|reason| format!(" — {reason}"))
                .unwrap_or_default(),
        ));
        out.push_str(&format!("  wall {wall}\n"));
    }
    let format_ms = |value: &Option<u64>| -> String {
        value
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "unknown".to_owned())
    };
    out.push_str(&format!(
        "scheduler wait {} · execution {} · expected {} · budget {}\n",
        format_ms(&record.timing.admission_wait_ms),
        format_ms(&record.timing.execution_wall_ms),
        format_ms(&record.timing.expected_wall_ms),
        record.budget_state.as_str(),
    ));
    out.push_str(&format!(
        "logs: .effigy/reports/qa-groups/{}/members · status: effigy tasks qa-group status {}\n",
        record.run_id, record.run_id
    ));
    if record.budget_state == QaGroupBudgetState::OverBudget {
        out.push_str(
            "budget: over_budget — the group's checks still own the outcome; over-budget evidence changes neither\n",
        );
    }
    for warning in &record.warnings {
        out.push_str(&format!("warning: {warning}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::build_member_request;
    use crate::runner::command_context::with_runtime_context;
    use effigy_context::EffigyRuntimeContext;
    use effigy_execution::ExecutionDispatchPlan;
    use effigy_tasks::QaGroupPlanMember;
    use std::path::PathBuf;

    #[test]
    fn qa_group_manifest_covers_shorthand_is_canonicalized_for_runner_payloads() {
        let cases = [
            (
                "covers = \"targets\"",
                serde_json::json!(["workspace:root"]),
            ),
            (
                "covers = [\"path:README.md\"]",
                serde_json::json!(["path:README.md"]),
            ),
            ("", serde_json::json!([])),
        ];

        for (covers, expected) in cases {
            let manifest = format!(
                r#"
[groups.example]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "required"
proof_limits = ["No broader claim"]

[[groups.example.members]]
id = "check"
kind = "test"
task = "test"
targets = ["workspace:root"]
{covers}
limits = ["Only this check"]
"#
            );
            let parsed: effigy_manifest::ManifestQaSection =
                toml::from_str(&manifest).expect("valid coverage form");
            let payload: Vec<String> = parsed.groups["example"].members[0]
                .covers
                .iter()
                .map(|token| token.as_token())
                .collect();
            let payload = serde_json::to_value(payload).expect("serialize canonical coverage");
            assert_eq!(payload, expected);
        }

        let invalid = r#"
[groups.example]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "required"
proof_limits = ["No broader claim"]

[[groups.example.members]]
id = "check"
kind = "test"
task = "test"
targets = ["workspace:root"]
covers = "all"
limits = ["Only this check"]
"#;
        let error = toml::from_str::<effigy_manifest::ManifestQaSection>(invalid)
            .expect_err("invalid shorthand must be refused");
        assert!(error.to_string().contains("string `targets`"), "{error}");
    }

    fn published_member() -> QaGroupPlanMember {
        QaGroupPlanMember {
            id: "t1".to_owned(),
            kind: "proof".to_owned(),
            surface: "published".to_owned(),
            catalog: "root".to_owned(),
            resolved_selector: "root/ok".to_owned(),
            task: "ok".to_owned(),
            args: Vec::new(),
            targets: vec!["workspace:root".to_owned()],
            covers: Vec::new(),
            companions: Vec::new(),
            limits: vec!["nothing".to_owned()],
            admission: "ordinary".to_owned(),
            heavy_reasons: Vec::new(),
            declared_run_in: "host".to_owned(),
        }
    }

    #[test]
    fn member_failure_keeps_the_producer_outcome_and_code() {
        use effigy_execution::{QaGroupMemberState, QaGroupOutcome};
        use effigy_kernel::{ctx::StopCause, outcome::WorkloadOutcome};
        let cancelled = crate::runner::error::RunnerError::task_json_failure(
            serde_json::json!({}),
            "check",
            130,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
            "",
            "stopped",
        );
        assert_eq!(
            super::classify_member_failure(&cancelled).0,
            QaGroupMemberState::Cancelled
        );
        assert_eq!(
            super::classify_group_failure(&cancelled),
            QaGroupOutcome::Cancelled
        );
        let failed = crate::runner::error::RunnerError::task_json_failure(
            serde_json::json!({}),
            "check",
            9,
            WorkloadOutcome::Failed { code: 9 },
            "",
            "failure",
        );
        let (state, _, code) = super::classify_member_failure(&failed);
        assert_eq!((state, code), (QaGroupMemberState::Failed, Some(9)));
        assert_eq!(
            super::classify_group_failure(&failed),
            QaGroupOutcome::Failed
        );
    }

    #[test]
    fn member_request_discovers_from_resolved_root_not_process_cwd() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().to_path_buf();
        let process_cwd = PathBuf::from("/effigy-qa-group-process-cwd");
        assert_ne!(
            root, process_cwd,
            "fixture root must differ from the process cwd"
        );
        let cli_context = EffigyRuntimeContext::capture_lossy(Some(process_cwd.clone()), None)
            .expect("cli context");
        assert!(cli_context.task_source().is_none());
        assert_eq!(cli_context.invocation_cwd(), process_cwd.as_path());

        let request = with_runtime_context(&cli_context, || {
            build_member_request(&root, &published_member())
        })
        .expect("member request");

        assert_eq!(request.environment.cwd.as_deref(), Some(root.as_path()));
        assert!(request.runtime_context.task_source().is_none());
        assert_eq!(request.runtime_context.invocation_cwd(), root.as_path());
        let plan = ExecutionDispatchPlan::from_request(request).expect("dispatch plan");
        assert_eq!(plan.effective_cwd, root);
        assert_ne!(plan.effective_cwd, process_cwd);
        assert_eq!(plan.selector, "root/ok");
    }
}
