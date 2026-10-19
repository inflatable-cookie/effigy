use std::fs;
use std::path::Path;

use chrono::Utc;
use effigy_execution::{
    ExecutionSurface, TaskStatusCompletedRecord, TaskStatusOutcome, TaskStatusRuntimeRouteSummary,
    TaskStatusStage, TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::task_status_latest_record_path;

use crate::runner::task_engine::TaskRun;
use crate::runner::tests::prelude::{
    assert_output_contains_all, parse_json_output_with_schema_version, run_task_in_workspace,
    run_task_status_all_from_repo, run_task_status_from_repo, temp_workspace, write_manifest,
    write_root_manifest,
};
use effigy_kernel::outcome::WorkloadOutcome;

fn canonical(root: &Path) -> std::path::PathBuf {
    fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

#[test]
fn run_tasks_status_json_reports_the_live_run() {
    let root = temp_workspace("tasks-status-json-live");
    write_root_manifest(&root, "[tasks.check]\nrun = \"printf check\"\n");
    let held = TaskRun::hold_for_tests(&canonical(&root), "check", None);

    let out = run_task_status_from_repo(&root, "check", true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v2", 2);
    assert_eq!(parsed["task"]["selector"], "check");
    assert_eq!(parsed["state"], "running");
    assert_eq!(parsed["current"]["pid"], std::process::id());
    assert!(parsed["latest"].is_null());
    assert_eq!(parsed["routing"]["selection_mode"], "cwd-nearest");
    held.close(WorkloadOutcome::Succeeded);

    let out = run_task_status_from_repo(&root, "check", true);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v2", 2);
    assert_eq!(parsed["state"], "succeeded");
    assert!(parsed["current"].is_null());
}

#[test]
fn run_tasks_status_text_reports_last_run_outcome() {
    let root = temp_workspace("tasks-status-text-latest");
    write_root_manifest(&root, "[tasks.check]\nrun = \"printf check\"\n");
    run_task_in_workspace(&root, "check", &[]).expect("run check");

    let out = run_task_status_from_repo(&root, "check", false);

    assert_output_contains_all(
        &out,
        &["Task Status", "check", "succeeded", "last-run", "exit-code"],
    );
}

#[test]
fn run_tasks_status_text_reports_never_run() {
    let root = temp_workspace("tasks-status-text-never-run");
    write_root_manifest(&root, "[tasks.check]\nrun = \"printf check\"\n");

    let out = run_task_status_from_repo(&root, "check", false);

    assert_output_contains_all(&out, &["never-run", "No runs recorded yet."]);
}

#[test]
fn run_tasks_status_shows_a_v014_record_read_only_until_the_first_run() {
    let root = temp_workspace("tasks-status-legacy");
    write_root_manifest(&root, "[tasks.check]\nrun = \"printf check\"\n");
    seed_latest_task_status(&root, "check");

    let out = run_task_status_from_repo(&root, "check", true);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v2", 2);
    assert_eq!(parsed["state"], "never-run");
    assert_eq!(parsed["legacy"]["schema"], "effigy.tasks-status.v1");
    let text = run_task_status_from_repo(&root, "check", false);
    assert_output_contains_all(&text, &["Effigy v0.14 recorded", "(read-only)"]);

    run_task_in_workspace(&root, "check", &[]).expect("run check");
    let out = run_task_status_from_repo(&root, "check", true);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v2", 2);
    assert_eq!(parsed["state"], "succeeded");
    assert!(parsed["legacy"].is_null());
}

#[test]
fn run_tasks_status_all_json_reports_declared_and_undeclared_live_rows() {
    let root = temp_workspace("tasks-status-all-json");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_root_manifest(
        &root,
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.check]\nrun = \"printf check\"\n[tasks.idle]\nrun = \"printf idle\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );
    run_task_in_workspace(&root, "catalog_a/build", &[]).expect("run build");
    let check = TaskRun::hold_for_tests(&canonical(&root), "check", None);
    let old = TaskRun::hold_for_tests(&canonical(&root), "old-task", None);

    let out = run_task_status_all_from_repo(&root, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status-all.v2", 2);
    let rows = parsed["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 4, "{rows:#?}");
    assert_eq!(parsed["counts_by_state"]["running"], 2);
    assert_eq!(parsed["counts_by_state"]["succeeded"], 1);
    assert_eq!(parsed["counts_by_state"]["never-run"], 1);
    assert!(rows.iter().any(|row| {
        row["selector"] == "check" && row["state"] == "running" && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "idle"
            && row["state"] == "never-run"
            && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "catalog_a/build"
            && row["state"] == "succeeded"
            && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "old-task"
            && row["state"] == "running"
            && row["currently_declared"] == false
    }));
    check.close(WorkloadOutcome::Succeeded);
    old.close(WorkloadOutcome::Succeeded);
}

#[test]
fn run_tasks_status_all_text_groups_rows_by_catalog_scope() {
    let root = temp_workspace("tasks-status-all-text");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_root_manifest(
        &root,
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.check]\nrun = \"printf check\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );
    run_task_in_workspace(&root, "catalog_a/build", &[]).expect("run build");

    let out = run_task_status_all_from_repo(&root, false);

    assert_output_contains_all(
        &out,
        &[
            "Task Status",
            "Catalog: catalog_a",
            "- check [never-run]",
            "- catalog_a/build [succeeded]",
        ],
    );
}

fn seed_latest_task_status(root: &Path, selector: &str) {
    let identity = status_identity(root, selector);
    let key = identity.status_key();
    let path = task_status_latest_record_path(root, &key);
    let record = completed_record(
        identity,
        key,
        path.clone(),
        TaskStatusState::Succeeded,
        "task completed",
    );
    write_json_file(&path, &record);
}

fn completed_record(
    identity: TaskStatusTargetIdentity,
    key: effigy_execution::TaskStatusKey,
    path: std::path::PathBuf,
    state: TaskStatusState,
    summary: &str,
) -> TaskStatusCompletedRecord {
    TaskStatusCompletedRecord {
        status_key: key,
        identity,
        state,
        stage: Some(TaskStatusStage::Finishing),
        execution_surface: ExecutionSurface::DirectCli,
        runtime_route: host_route(),
        started_at: timestamp_now(),
        finished_at: timestamp_now(),
        duration_ms: Some(42),
        lock_scopes: vec!["task:test".to_owned()],
        outcome: TaskStatusOutcome {
            summary: summary.to_owned(),
            error_family: None,
            error_code: None,
        },
        latest_report_path: path.display().to_string(),
        history_report_path: path
            .parent()
            .expect("latest parent")
            .join("history-placeholder.json")
            .display()
            .to_string(),
    }
}

fn status_identity(root: &Path, selector: &str) -> TaskStatusTargetIdentity {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let selected_catalog_root = selector
        .split_once('/')
        .map(|(prefix, _)| canonical_root.join(prefix))
        .unwrap_or_else(|| canonical_root.clone());
    TaskStatusTargetIdentity::new(
        canonical_root.clone(),
        selected_catalog_root,
        selector,
        selector.rsplit('/').next().unwrap_or(selector),
        None,
    )
}

fn host_route() -> TaskStatusRuntimeRouteSummary {
    TaskStatusRuntimeRouteSummary {
        route: "host".to_owned(),
        container: None,
        service: None,
    }
}

fn timestamp_now() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn write_json_file(path: &Path, value: &impl serde::Serialize) {
    fs::create_dir_all(path.parent().expect("status parent")).expect("create status dir");
    fs::write(
        path,
        serde_json::to_vec_pretty(value).expect("encode status record"),
    )
    .expect("write status record");
}
