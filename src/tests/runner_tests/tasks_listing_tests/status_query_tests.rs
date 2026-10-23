use std::fs;
use std::path::Path;

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

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v3", 3);
    assert_eq!(parsed["task"]["selector"], "check");
    assert_eq!(parsed["state"], "running");
    assert_eq!(parsed["current"]["pid"], std::process::id());
    assert!(parsed["latest"].is_null());
    assert_eq!(parsed["routing"]["selection_mode"], "cwd-nearest");
    held.close(WorkloadOutcome::Succeeded);

    let out = run_task_status_from_repo(&root, "check", true);
    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status.v3", 3);
    assert_eq!(parsed["state"], "not-recorded");
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
        &[
            "Task Status",
            "check",
            "not-recorded",
            "No runs recorded yet.",
        ],
    );
}

#[test]
fn run_tasks_status_text_reports_never_run() {
    let root = temp_workspace("tasks-status-text-never-run");
    write_root_manifest(&root, "[tasks.check]\nrun = \"printf check\"\n");

    let out = run_task_status_from_repo(&root, "check", false);

    assert_output_contains_all(&out, &["not-recorded", "No runs recorded yet."]);
    let parsed = parse_json_output_with_schema_version(
        &run_task_status_from_repo(&root, "check", true),
        "effigy.tasks-status.v3",
        3,
    );
    // v0.14 status records are retired, not read: a task with no kernel run
    // rows carries no legacy view at all (contract 052 §10.1).
    assert!(parsed.get("legacy").is_none(), "{parsed}");
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

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks-status-all.v3", 3);
    let rows = parsed["rows"].as_array().expect("rows array");
    assert_eq!(rows.len(), 4, "{rows:#?}");
    assert_eq!(parsed["counts_by_state"]["running"], 2);
    assert_eq!(parsed["counts_by_state"]["not-recorded"], 2);
    assert!(rows.iter().any(|row| {
        row["selector"] == "check" && row["state"] == "running" && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "idle"
            && row["state"] == "not-recorded"
            && row["currently_declared"] == true
    }));
    assert!(rows.iter().any(|row| {
        row["selector"] == "catalog_a/build"
            && row["state"] == "not-recorded"
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
            "- check [not-recorded]",
            "- catalog_a/build [not-recorded]",
        ],
    );
}
