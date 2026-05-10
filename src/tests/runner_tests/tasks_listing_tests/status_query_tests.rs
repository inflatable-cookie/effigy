use std::fs;
use std::path::Path;

use chrono::Utc;
use effigy_execution::{
    ExecutionSurface, TaskStatusActiveRecord, TaskStatusCompletedRecord, TaskStatusOutcome,
    TaskStatusRuntimeRouteSummary, TaskStatusStage, TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::{task_status_active_record_path, task_status_latest_record_path};

use crate::runner::tests::prelude::{
    assert_output_contains_all, parse_json_output_with_schema_version, run_task_status_from_repo,
    temp_workspace, write_root_manifest,
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
    assert_eq!(parsed["routing"]["selection_mode"], "root-shallowest");
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
    let record = TaskStatusCompletedRecord {
        status_key: key,
        identity,
        state: TaskStatusState::Succeeded,
        stage: Some(TaskStatusStage::Finishing),
        execution_surface: ExecutionSurface::DirectCli,
        runtime_route: host_route(),
        started_at: timestamp_now(),
        finished_at: timestamp_now(),
        duration_ms: Some(42),
        lock_scopes: vec!["task:test".to_owned()],
        outcome: TaskStatusOutcome {
            summary: "task completed".to_owned(),
            error_family: None,
            error_code: None,
        },
        latest_report_path: path.display().to_string(),
        history_report_path: root
            .join(".effigy/reports/tasks/history-placeholder.json")
            .display()
            .to_string(),
    };
    write_json_file(&path, &record);
}

fn status_identity(root: &Path, selector: &str) -> TaskStatusTargetIdentity {
    TaskStatusTargetIdentity::new(
        root.to_path_buf(),
        root.to_path_buf(),
        selector,
        selector,
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
