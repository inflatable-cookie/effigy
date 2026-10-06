//! QA-group run coordinator: one run identity, sequential member execution
//! through the canonical pipeline, one durable ledger.
//!
//! Members run in declaration order, one at a time, each as a typed
//! `TaskExecutionRequest` handed to the ordinary pipeline (routing,
//! environment, secrets, containers, locks, and cleanup stay pipeline
//! responsibilities). A scheduler-launched group owns its children through
//! the validated parent token, so a heavy member never waits behind itself or
//! submits a second run. A failed
//! member ends the group; later members are recorded `not_started` and no
//! pass is synthesized. Expected wall time is compared with execution wall
//! time only; over-budget evidence never changes the check outcome.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use chrono::Utc;
use effigy_execution::{
    QaGroupBackend, QaGroupBudgetState, QaGroupGapSnapshot, QaGroupMemberRecord,
    QaGroupMemberState, QaGroupOutcome, QaGroupRunCapabilities, QaGroupRunGroupSnapshot,
    QaGroupRunHead, QaGroupRunRecord, QaGroupRunState, QaGroupRunTiming, QA_GROUP_RUN_SCHEMA,
};
use effigy_tasks::{budget_state, QaGroupPlan, QaGroupPlanMember};

use super::super::error::RunnerError;
use super::super::execute::api::run_manifest_task_request;
use super::super::host_scheduler::{self, Route, SubmitContext};
use super::super::owned_children::OwnedChildrenScope;
use super::member_runtime_context;
use effigy_runtime::qa_group_status::{
    begin_qa_group_run_record, finalize_qa_group_run_record, update_qa_group_run_record,
    write_qa_group_member_log,
};

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Captured output from one member attempt. The typed pipeline error travels
/// unchanged so cancelled versus failed versus blocked stay distinguishable;
/// the error is boxed to keep the `Err` variant small.
type MemberAttempt = Result<(String, String), (Box<RunnerError>, String, String)>;

pub(super) fn execute_group_run(
    root: &Path,
    invocation_cwd: &Path,
    plan: &QaGroupPlan,
    selector_env_names: std::collections::BTreeSet<String>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let run_id = generate_run_id();
    let heavy = plan.admission.required;
    let selector = format!("qa-group:{}/{}", plan.group.catalog, plan.group.name);
    // Routing decisions that can refuse (invalid setting, forged token, down
    // scheduler) happen here, before any ledger entry or member effect.
    let route = if heavy {
        Some(host_scheduler::route_heavy(&selector, invocation_cwd)?)
    } else {
        None
    };
    if route == Some(Route::Submit) {
        return submit_group_run(
            root,
            invocation_cwd,
            plan,
            selector_env_names,
            output_json,
            &run_id,
            &selector,
        );
    }
    let run_mode = if heavy {
        RunMode::Owned
    } else {
        RunMode::Light
    };
    let mut record = initial_record(&run_id, plan);
    record.backend = route.as_ref().and_then(|route| backend_for(route, heavy));
    record.timing.admission_wait_ms = record
        .backend
        .as_ref()
        .and_then(|backend| backend.queue_wait_ms);

    begin_qa_group_run_record(root, &record)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    let outcome = run_members(root, invocation_cwd, plan, &mut record, run_mode);
    let rendered = if output_json {
        render_record_json(&record)?
    } else {
        render_record_text(&record)
    };

    match outcome {
        Ok(()) => {
            finalize_qa_group_run_record(root, &record)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            Ok(rendered)
        }
        Err(_) => {
            finalize_qa_group_run_record(root, &record)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
            // The final ledger is the failure payload: the caller sees the
            // honest record with a failing exit, never a bare message.
            Err(RunnerError::CommandJsonFailure { rendered })
        }
    }
}

/// Whether this group has a scheduler-owned parent scope.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RunMode {
    /// No heavy members: direct execution.
    Light,
    /// Already covered: a validated scheduler run, or a recorded override.
    Owned,
}

/// Backend correlation for a scheduler-covered heavy run.
fn backend_for(route: &Route, heavy: bool) -> Option<QaGroupBackend> {
    if !heavy {
        return None;
    }
    match route {
        Route::Nested(nested) => Some(QaGroupBackend {
            kind: "host_scheduler".to_owned(),
            scheduler_run_id: Some(nested.run_id.clone()),
            scheduler_epoch: Some(nested.epoch),
            queue_wait_ms: host_scheduler::nested_queue_wait_ms(nested),
            settlement: None,
        }),
        Route::Override => Some(QaGroupBackend {
            kind: "host_scheduler_override".to_owned(),
            scheduler_run_id: None,
            scheduler_epoch: None,
            queue_wait_ms: None,
            settlement: None,
        }),
        Route::Submit => None,
    }
}

/// Top-level scheduler path for a heavy group. The scheduler launches this same
/// invocation as the owner of the group ledger, so a run that launched is
/// followed to its real status and writes no second record here. Only a run the
/// scheduler settled without launching leaves a record, written by this process.
fn submit_group_run(
    root: &Path,
    invocation_cwd: &Path,
    plan: &QaGroupPlan,
    selector_env_names: std::collections::BTreeSet<String>,
    output_json: bool,
    run_id: &str,
    selector: &str,
) -> Result<String, RunnerError> {
    let settled = host_scheduler::submit_and_settle(SubmitContext {
        selector,
        class_source: effigy_host_run::ClassSource::Manifest,
        repository: root,
        cwd: invocation_cwd,
        selector_env_names,
    })?;
    let host_scheduler::Settled::NotLaunched {
        run_id: scheduler_run_id,
        epoch,
        reason,
        queue_wait_ms,
        interrupt_signal,
    } = settled.clone()
    else {
        return Err(settled.into_error());
    };
    let (outcome, label) = match reason {
        host_scheduler::PreLaunch::CapacityTimeout => {
            (QaGroupOutcome::CapacityTimeout, "capacity_timeout")
        }
        host_scheduler::PreLaunch::Cancelled => (QaGroupOutcome::Cancelled, "cancelled"),
    };
    let mut record = initial_record(run_id, plan);
    record.state = QaGroupRunState::Completed;
    record.outcome = Some(outcome);
    record.timing.queued_at = Some(Utc::now().to_rfc3339());
    record.timing.ended_at = Some(Utc::now().to_rfc3339());
    record.timing.admission_wait_ms = queue_wait_ms;
    record.backend = Some(QaGroupBackend {
        kind: "host_scheduler".to_owned(),
        scheduler_run_id: Some(scheduler_run_id.clone()),
        scheduler_epoch: Some(epoch),
        queue_wait_ms,
        settlement: Some(label.to_owned()),
    });
    record.warnings.push(format!(
        "host scheduler run {scheduler_run_id} settled as {label} before any member launched"
    ));
    touch(&mut record);
    begin_qa_group_run_record(root, &record)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    finalize_qa_group_run_record(root, &record)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let rendered = if output_json {
        render_record_json(&record)?
    } else {
        render_record_text(&record)
    };
    match interrupt_signal {
        Some(signal) => {
            println!("{rendered}");
            Err(RunnerError::HostRunSettled { code: 128 + signal })
        }
        None => Err(RunnerError::CommandJsonFailure { rendered }),
    }
}

/// Run members serially and finalize aggregate states.
fn run_members(
    root: &Path,
    invocation_cwd: &Path,
    plan: &QaGroupPlan,
    record: &mut QaGroupRunRecord,
    run_mode: RunMode,
) -> Result<(), RunnerError> {
    record.timing.queued_at = Some(Utc::now().to_rfc3339());

    let _owned_children = if run_mode == RunMode::Owned {
        Some(OwnedChildrenScope::enter().map_err(|error| {
            RunnerError::task_invocation(format!(
                "cannot install heavy-run signal forwarding: {error}"
            ))
        })?)
    } else {
        None
    };

    record.state = QaGroupRunState::Running;
    record.timing.started_at = Some(Utc::now().to_rfc3339());
    touch(record);
    let _ = update_qa_group_run_record(root, record);

    let started = Instant::now();
    let result = execute_member_loop(root, invocation_cwd, plan, record);
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
    invocation_cwd: &Path,
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
        let attempt: MemberAttempt = run_single_member(root, invocation_cwd, member);
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
fn run_single_member(
    root: &Path,
    invocation_cwd: &Path,
    member: &QaGroupPlanMember,
) -> MemberAttempt {
    let request = match build_member_request(root, invocation_cwd, member) {
        Ok(request) => request,
        Err(error) => return Err((Box::new(error), String::new(), String::new())),
    };

    match run_manifest_task_request(request) {
        Ok(rendered) => {
            let (stdout, stderr) = extract_payload_output(&rendered);
            Ok((stdout, stderr))
        }
        Err(RunnerError::CommandJsonFailure { rendered }) => {
            // The captured payload carries the task's real exit code and its
            // redacted output. Re-derive the typed failure from them so the
            // ledger can distinguish cancellation (exit 130) from failure
            // and record the actual code; the group renders its own ledger,
            // so the wrapped JSON payload is not needed downstream.
            let (stdout, stderr, exit_code) = extract_failure_payload(&rendered);
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
    invocation_cwd: &Path,
    member: &QaGroupPlanMember,
) -> Result<effigy_execution::TaskExecutionRequest, RunnerError> {
    let mut args = member.args.clone();
    args.push("--json".to_owned());
    effigy_execution::TaskExecutionRequestBuilder::new()
        .runtime_context(member_runtime_context(root, invocation_cwd)?)
        .task(format!("{}/{}", member.catalog, member.task), args)
        .surface(if member.surface == "draft" {
            effigy_execution::ExecutionSurface::Draft
        } else {
            effigy_execution::ExecutionSurface::QaGroup
        })
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

fn extract_failure_payload(rendered: &str) -> (String, String, i32) {
    match serde_json::from_str::<serde_json::Value>(rendered) {
        Ok(value) => (
            string_field(&value, "stdout"),
            string_field(&value, "stderr"),
            value
                .get("exit_code")
                .and_then(|code| code.as_i64())
                .and_then(|code| i32::try_from(code).ok())
                .unwrap_or(1),
        ),
        Err(_) => (String::new(), String::new(), 1),
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
        _ => (String::new(), String::new()),
    }
}

fn classify_member_failure(error: &RunnerError) -> (QaGroupMemberState, String, Option<i32>) {
    match error {
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
        RunnerError::CommandJsonFailure { .. } => (
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
        coverage_disclaimer: effigy_tasks::SCOPE_COVERAGE_DISCLAIMER.to_owned(),
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
        capabilities: QaGroupRunCapabilities {
            hard_timeout: false,
            stop: false,
        },
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

fn generate_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "qa-{}-{nanos}-{}",
        std::process::id(),
        RUN_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
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
