//! `effigy tasks qa-groups` / `effigy tasks qa-group` command surfaces.
//!
//! List, plan, run, status, and logs live here; they compose the pure group
//! surface in `effigy-tasks` with the canonical execution pipeline. Stop is
//! parsed but always rejected before any side effect: run-scoped stop and
//! signal attribution (lead `29e5f6f7`) has not landed, and faking it would
//! be worse than refusing.

mod execute;

use std::path::{Path, PathBuf};

use effigy_cli::{TasksArgs, TasksQaCommand};
use effigy_context::EffigyRuntimeContext;
use effigy_execution::QaGroupStatusSnapshot;
use effigy_manifest::ManifestDraftDate;
use effigy_routing::load_effective_catalogs_allow_missing;
use effigy_runtime::qa_group_status::{list_qa_group_run_ids, load_qa_group_run_record};
use effigy_tasks::{
    build_qa_group_plan, list_qa_groups, render_qa_group_plan_json, render_qa_group_plan_text,
    render_qa_groups_json, render_qa_groups_text, select_qa_group, ListQaGroupsRequest,
    QaFileTracking, QaGroupSelectionRequest, ScopeAssessment,
};

use super::command_context::resolve_active_command_context;
use super::error::RunnerError;

pub(super) fn run_tasks_qa(args: &TasksArgs, qa: &TasksQaCommand) -> Result<String, RunnerError> {
    // The global `--json` flag and a command-local `--json` both select the
    // versioned payload; `--json` is output format only and never changes
    // execute-versus-plan.
    let json_output = args.output_json;
    match qa {
        TasksQaCommand::GroupsList {
            filter,
            file,
            output_json,
            pretty_json,
        } => run_qa_groups_list(
            args,
            filter.as_deref(),
            file.as_deref(),
            *output_json || json_output,
            *pretty_json,
        ),
        TasksQaCommand::GroupRun {
            selector,
            file,
            scopes,
            plan,
            output_json,
        } => run_qa_group(
            args,
            selector,
            file.as_deref(),
            scopes,
            *plan,
            *output_json || json_output,
        ),
        TasksQaCommand::GroupStatus { run_id, output_json } => {
            run_qa_group_status(args, run_id, *output_json || json_output)
        }
        TasksQaCommand::GroupLogs { run_id, follow } => {
            run_qa_group_logs(args, run_id, *follow)
        }
        TasksQaCommand::GroupStop { run_id, .. } => Err(RunnerError::task_invocation(format!(
            "`tasks qa-group stop {run_id}` is not available: run-scoped stop and signal attribution (lead 29e5f6f7) has not landed, so no supervisor can confirm the run's process tree stopped. Nothing was signalled and no state changed. Ask the planner if the run needs operator intervention."
        ))),
    }
}

fn run_qa_groups_list(
    args: &TasksArgs,
    filter: Option<&str>,
    file: Option<&Path>,
    output_json: bool,
    pretty_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let catalogs = load_effective_catalogs_allow_missing(&context.resolved.resolved_root)?;
    let tracking = file
        .map(|path| git_file_tracking(&context.resolved.resolved_root, path))
        .unwrap_or(QaFileTracking::Unknown);
    let listing = list_qa_groups(
        ListQaGroupsRequest {
            filter,
            file,
            file_tracking: tracking,
            pretty_json,
            resolved_root: &context.resolved.resolved_root,
            today: ManifestDraftDate::today_local(),
        },
        &catalogs,
    )
    .map_err(map_tasks_error)?;
    if output_json {
        render_qa_groups_json(&listing, pretty_json).map_err(map_tasks_error)
    } else {
        render_qa_groups_text(&listing).map_err(map_tasks_error)
    }
}

/// Resolve the plan for `tasks qa-group run`. `needs_planner` blocks run
/// creation here: the caller gets the plan back with a failing exit and no
/// run record exists.
fn resolve_run_plan(
    args: &TasksArgs,
    selector: &str,
    file: Option<&Path>,
    scopes: &[String],
) -> Result<(std::path::PathBuf, effigy_tasks::QaGroupPlan), RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let root = context.resolved.resolved_root.clone();
    let catalogs = load_effective_catalogs_allow_missing(&root)?;
    let tracking = file
        .map(|path| git_file_tracking(&root, path))
        .unwrap_or(QaFileTracking::Unknown);
    let selected = select_qa_group(QaGroupSelectionRequest {
        selector,
        file,
        catalogs: &catalogs,
        invocation_cwd: &context.invocation_cwd,
        resolved_root: &root,
        file_tracking: tracking,
    })
    .map_err(map_tasks_error)?;
    let mut plan = build_qa_group_plan(effigy_tasks::QaGroupPlanRequest {
        selected: &selected,
        catalogs: &catalogs,
        scope_tokens: scopes,
        today: ManifestDraftDate::today_local(),
    })
    .map_err(map_tasks_error)?;
    let head = git_head_context(&root);
    plan.head = effigy_tasks::QaGroupHeadContext {
        commit: head.0,
        worktree: head.1,
    };
    Ok((root, plan))
}

fn run_qa_group(
    args: &TasksArgs,
    selector: &str,
    file: Option<&Path>,
    scopes: &[String],
    plan_only: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let (root, plan) = resolve_run_plan(args, selector, file, scopes)?;
    let render_json = |plan: &effigy_tasks::QaGroupPlan| {
        render_qa_group_plan_json(plan, true).map_err(map_tasks_error)
    };
    let render_text = |plan: &effigy_tasks::QaGroupPlan| {
        render_qa_group_plan_text(plan).map_err(map_tasks_error)
    };

    if plan.scope_assessment == ScopeAssessment::NeedsPlanner {
        // A needs_planner plan is not executable and has no run ID; an
        // attempted run returns the same plan with a failing exit.
        let rendered = if output_json { render_json(&plan)? } else { render_text(&plan)? };
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    if plan_only {
        return if output_json { render_json(&plan) } else { render_text(&plan) };
    }
    execute::execute_group_run(&root, &plan, output_json)
}

fn run_qa_group_status(
    args: &TasksArgs,
    run_id: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let root = context.resolved.resolved_root;
    let snapshot = load_qa_group_run_record(&root, run_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "no qa-group run named `{run_id}` in this repository; run IDs look like `qa-<pid>-<nanos>-<n>` and are printed when a group starts"
            ))
        })?;
    if output_json {
        render_status_json(&snapshot)
    } else {
        render_status_text(&snapshot)
    }
}

fn run_qa_group_logs(args: &TasksArgs, run_id: &str, follow: bool) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let root = context.resolved.resolved_root;
    let snapshot = load_qa_group_run_record(&root, run_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
        .ok_or_else(|| {
            RunnerError::task_invocation(format!("no qa-group run named `{run_id}` in this repository"))
        })?;
    let mut output = String::new();
    output.push_str(&format!(
        "qa-group run {run_id}: {} (definition {})\n",
        snapshot.record.state.as_str(),
        snapshot
            .record
            .group
            .definition_sha256
            .as_deref()
            .unwrap_or("composed manifest")
    ));
    for member in &snapshot.record.members {
        output.push_str(&format!(
            "\n=== member {} [{}] ===\n",
            member.id,
            member.state.as_str()
        ));
        match &member.log_ref {
            Some(log_ref) => {
                let path = root.join(log_ref);
                match std::fs::read_to_string(&path) {
                    Ok(content) => output.push_str(&content),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        output.push_str(&format!("(log not written yet: {log_ref})\n"));
                    }
                    Err(error) => {
                        return Err(RunnerError::task_invocation(format!(
                            "failed to read member log `{}`: {error}",
                            path.display()
                        )))
                    }
                }
            }
            None => output.push_str("(no log; member has not run)\n"),
        }
    }
    for warning in &snapshot.warnings {
        output.push_str(&format!("warning: {warning}\n"));
    }
    if follow && snapshot.live {
        output.push_str(&format!(
            "(live record; re-run `tasks qa-group logs {run_id}` to see member output as it lands)\n"
        ));
    }
    Ok(output)
}

fn render_status_json(snapshot: &QaGroupStatusSnapshot) -> Result<String, RunnerError> {
    let payload = serde_json::json!({
        "schema": effigy_execution::QA_GROUP_STATUS_SCHEMA,
        "schema_version": 1,
        "run_id": snapshot.run_id,
        "record_path": snapshot.record_path,
        "live": snapshot.live,
        "warnings": snapshot.warnings,
        "run": snapshot.record,
    });
    effigy_ui::encode_json(&payload, true).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

fn render_status_text(snapshot: &QaGroupStatusSnapshot) -> Result<String, RunnerError> {
    let record = &snapshot.record;
    let mut out = String::new();
    out.push_str(&format!(
        "QA group run {} ({})\n",
        record.run_id,
        record.state.as_str()
    ));
    out.push_str(&format!(
        "group: {}/{} ({}, catalog {})\n",
        record.group.catalog, record.group.name, record.group.lifecycle, record.group.catalog
    ));
    out.push_str(&format!("source: {}\n", record.group.source));
    if let Some(digest) = &record.group.definition_sha256 {
        out.push_str(&format!("definition digest: {digest}\n"));
    }
    let head_commit = record
        .head
        .commit
        .clone()
        .unwrap_or_else(|| "unknown".to_owned());
    out.push_str(&format!("head: {head_commit} ({})\n", record.head.worktree));
    let timing = &record.timing;
    let format_ms = |value: &Option<u64>| -> String {
        value
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "unknown".to_owned())
    };
    out.push_str(&format!(
        "timing: admission wait {}, execution {}, expected {}\n",
        format_ms(&timing.admission_wait_ms),
        format_ms(&timing.execution_wall_ms),
        format_ms(&timing.expected_wall_ms),
    ));
    out.push_str(&format!(
        "budget: {} · outcome: {}\n",
        record.budget_state.as_str(),
        record
            .outcome
            .map(|outcome| outcome.as_str())
            .unwrap_or("incomplete"),
    ));
    for member in &record.members {
        let wall = member
            .wall_ms
            .map(|ms| format!("{ms} ms"))
            .unwrap_or_else(|| "unknown".to_owned());
        out.push_str(&format!(
            "- {} [{}]{} wall {}{}\n",
            member.id,
            member.state.as_str(),
            member
                .summary
                .as_deref()
                .map(|summary| format!(" ({summary})"))
                .unwrap_or_default(),
            wall,
            member
                .not_started_reason
                .as_deref()
                .map(|reason| format!(" — {reason}"))
                .unwrap_or_default(),
        ));
    }
    for warning in &snapshot.warnings {
        out.push_str(&format!("warning: {warning}\n"));
    }
    out.push_str(&format!(
        "logs: effigy tasks qa-group logs {} · status --json for the full payload\n",
        record.run_id
    ));
    Ok(out)
}

fn map_tasks_error(error: effigy_tasks::EffigyTasksError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

/// `(commit, worktree state)` for the selected repository. Git identity is
/// context, never proof of clean inputs; failures degrade to `unknown`.
pub(super) fn git_head_context(root: &Path) -> (Option<String>, String) {
    let commit = git_output(root, &["rev-parse", "HEAD"]).map(|value| value.trim().to_owned());
    let dirty = git_output(root, &["status", "--porcelain"]).is_some_and(|value| {
        !value.trim().is_empty()
    });
    let worktree = if dirty { "modified" } else { "clean" }.to_owned();
    (commit.filter(|value| !value.is_empty()), worktree)
}

/// How Git sees one file, for temporary-definition provenance.
pub(super) fn git_file_tracking(root: &Path, file: &Path) -> QaFileTracking {
    match git_output(root, &["ls-files", "--error-unmatch", &file.display().to_string()]) {
        Some(_) => QaFileTracking::Tracked,
        None => match git_output(root, &["status", "--porcelain"]) {
            Some(_) => QaFileTracking::Untracked,
            None => QaFileTracking::Unknown,
        },
    }
}

fn git_output(root: &Path, git_args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(git_args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// Runtime context for member execution requests: the active captured
/// context when present (embedded dispatch), otherwise a capture rooted at
/// the resolved repository.
pub(super) fn member_runtime_context(root: &Path) -> Result<EffigyRuntimeContext, RunnerError> {
    super::command_context::active_runtime_context()
        .filter(|context| context.task_source().is_some())
        .map(Ok)
        .unwrap_or_else(|| {
            EffigyRuntimeContext::capture_lossy(Some(root.to_path_buf()), None)
                .map_err(|error| RunnerError::task_invocation(error.to_string()))
        })
}

/// Unused today but kept typed for the status inventory surface planned in
/// guide 081; prevents accidental widening of the record schema.
#[allow(dead_code)]
fn _unused(_: Vec<PathBuf>) {}

/// Referenced so the run inventory stays reachable for tests.
#[allow(dead_code)]
pub(super) fn known_run_ids(root: &Path) -> Result<Vec<String>, RunnerError> {
    list_qa_group_run_ids(root).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

/// Load helper used by tests of the command glue.
#[allow(dead_code)]
pub(super) fn load_record(root: &Path, run_id: &str) -> Result<Option<QaGroupStatusSnapshot>, RunnerError> {
    load_qa_group_run_record(root, run_id)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))
}
