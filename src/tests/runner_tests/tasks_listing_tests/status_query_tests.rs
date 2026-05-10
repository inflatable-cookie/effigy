use std::fs;
use std::path::Path;

use chrono::Utc;
use effigy_execution::{
    ExecutionSurface, TaskStatusActiveRecord, TaskStatusCompletedRecord, TaskStatusOutcome,
    TaskStatusRuntimeRouteSummary, TaskStatusStage, TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::{task_status_active_record_path, task_status_latest_record_path};

use crate::runner::tests::prelude::{
    assert_output_contains_all, parse_json_output_with_schema_version,
    run_task_status_all_from_repo, run_task_status_from_repo, temp_workspace, write_manifest,
    write_root_manifest,
};

#[test]
fn run_tasks_status_json_prefers_live_active_record() {
    let root = temp_workspace("tasks-status-json-active");
    write_root_manifest(&root, "[tasks.test]\nrun = \"printf test\"\n");
    seed_active_task_status(&root, "test");

    let out = run_task_status_from_repo(&root, "test", true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v1", 1);
    assert_eq!(parsed["resolved_selector"], "test");
    assert_eq!(parsed["state"], "running");
    assert_eq!(parsed["currently_declared"], true);
    assert_eq!(parsed["active"]["stage"], "executing");
    assert!(parsed["latest"].is_null());
    assert_eq!(parsed["routing"]["selection_mode"], "cwd-nearest");
}

#[test]
fn run_tasks_status_text_reports_last_known_completed_outcome() {
    let root = temp_workspace("tasks-status-text-latest");
    write_root_manifest(&root, "[tasks.test]\nrun = \"printf test\"\n");
    seed_latest_task_status(&root, "test");

    let out = run_task_status_from_repo(&root, "test", false);

    assert_output_contains_all(
        &out,
        &["Task Status", "test", "succeeded", "task completed", "host"],
    );
}

#[test]
fn run_tasks_status_text_reports_unknown_when_declared_task_has_no_records() {
    let root = temp_workspace("tasks-status-text-unknown");
    write_root_manifest(&root, "[tasks.test]\nrun = \"printf test\"\n");

    let out = run_task_status_from_repo(&root, "test", false);

    assert_output_contains_all(&out, &["unknown", "No recorded task status yet."]);
}

#[test]
fn run_tasks_status_all_json_reports_declared_and_stale_rows() {
    let root = temp_workspace("tasks-status-all-json");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_root_manifest(
        &root,
        "[tasks.test]\nrun = \"printf test\"\n[tasks.idle]\nrun = \"printf idle\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );
    seed_active_task_status(&root, "test");
    seed_latest_task_status(&root, "catalog_a/build");
    seed_undeclared_latest_task_status(&root, "old-task");

    let out = run_task_status_all_from_repo(&root, true);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status-all.v1", 1);
    let rows = parsed["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 4);
    assert_eq!(parsed["counts_by_state"]["running"], 1);
    assert_eq!(parsed["counts_by_state"]["succeeded"], 1);
    assert_eq!(parsed["counts_by_state"]["unknown"], 1);
    assert_eq!(parsed["counts_by_state"]["failed"], 1);
    assert!(rows.iter().any(|row| {
        row["selector"] == "test" && row["state"] == "running" && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "idle" && row["state"] == "unknown" && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "catalog_a/build"
            && row["state"] == "succeeded"
            && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "old-task"
            && row["state"] == "failed"
            && row["currently_declared"] == false
            && row["no_longer_declared"] == true
    }));
}

#[test]
fn run_tasks_status_all_text_groups_rows_by_catalog_scope() {
    let root = temp_workspace("tasks-status-all-text");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_root_manifest(&root, "[tasks.test]\nrun = \"printf test\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );
    seed_latest_task_status(&root, "catalog_a/build");

    let out = run_task_status_all_from_repo(&root, false);

    assert_output_contains_all(
        &out,
        &[
            "Task Status",
            "Catalog: root",
            "Catalog: catalog_a",
            "- test [unknown]",
            "- catalog_a/build [succeeded] host",
        ],
    );
}

fn seed_active_task_status(root: &Path, selector: &str) {
    let identity = status_identity(root, selector);
    let key = identity.status_key();
    let path = task_status_active_record_path(root, &key);
    let record = TaskStatusActiveRecord {
        status_key: key,
        identity,
        state: TaskStatusState::Running,
        stage: TaskStatusStage::Executing,
        execution_surface: ExecutionSurface::DirectCli,
        runtime_route: host_route(),
        owner_pid: std::process::id(),
        started_at: timestamp_now(),
        updated_at: timestamp_now(),
        lock_scopes: vec!["task:test".to_owned()],
        active_record_path: path.display().to_string(),
    };
    write_json_file(&path, &record);
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

fn seed_undeclared_latest_task_status(root: &Path, selector: &str) {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let identity = TaskStatusTargetIdentity::new(
        canonical_root.clone(),
        canonical_root,
        selector,
        selector,
        None,
    );
    let key = identity.status_key();
    let path = task_status_latest_record_path(root, &key);
    let record = completed_record(
        identity,
        key,
        path.clone(),
        TaskStatusState::Failed,
        "old task failed",
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
