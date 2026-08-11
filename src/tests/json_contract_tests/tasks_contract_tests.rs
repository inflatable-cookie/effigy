use crate::runner::json_contract_tests::prelude::{harness::*, json::*, runtime::*};
use chrono::Utc;
use effigy_execution::{
    ExecutionSurface, TaskStatusCompletedRecord, TaskStatusOutcome, TaskStatusRuntimeRouteSummary,
    TaskStatusStage, TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::task_status_latest_record_path;
use std::path::Path;

#[test]
fn tasks_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-json-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["managed_profiles"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn tasks_filtered_json_contract_has_versioned_shape_and_filter_fields() {
    let root = temp_workspace("tasks-filtered-json-contract");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["notes"].is_array());
}

#[test]
fn tasks_json_contract_catalog_payload_uses_expected_top_level_fields() {
    let root = temp_workspace("tasks-json-contract-top-level-catalog");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert_eq!(
        sorted_object_keys(&parsed),
        vec![
            "builtin_tasks",
            "catalog_count",
            "catalog_tasks",
            "catalogs",
            "managed_profiles",
            "precedence",
            "resolve",
            "schema",
            "schema_version",
        ]
    );
}

#[test]
fn tasks_json_contract_filtered_payload_uses_expected_top_level_fields() {
    let root = temp_workspace("tasks-json-contract-top-level-filtered");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(
        sorted_object_keys(&parsed),
        vec![
            "builtin_matches",
            "catalog_count",
            "catalogs",
            "filter",
            "managed_profile_matches",
            "matches",
            "notes",
            "precedence",
            "resolve",
            "schema",
            "schema_version",
        ]
    );
}

#[test]
fn tasks_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-json-contract-resolve");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: Some("catalog_a/api".to_owned()),
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.v1");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "catalog_a");
    assert_eq!(parsed["resolve"]["task"], "api");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "task:catalog_a/api");
}

#[test]
fn tasks_filtered_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-filtered-json-contract-resolve");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("build".to_owned()),
            resolve_selector: Some("catalog_a/build".to_owned()),
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json resolve");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "build");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "catalog_a");
    assert_eq!(parsed["resolve"]["task"], "build");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "task:catalog_a/build");
}

#[test]
fn tasks_json_contract_excludes_explicitly_deferred_builtins() {
    let root = temp_workspace("tasks-json-contract-hidden-deferred-builtin");
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed = parse_json(&out);
    let builtin_tasks = parsed["builtin_tasks"].as_array().expect("builtin_tasks");
    assert!(!builtin_tasks.iter().any(|item| item["task"] == "release"));
    assert!(builtin_tasks.iter().any(|item| item["task"] == "doctor"));
}

#[test]
fn tasks_status_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-status-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.check]\nrun = \"printf check\"\n",
    );
    seed_latest_task_status(&root, "check");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: Some("check".to_owned()),
            status_all: false,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run task status json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks-status.v1");
    assert_eq!(parsed["resolved_selector"], "check");
    assert_eq!(parsed["state"], "succeeded");
    assert!(parsed["active"].is_null());
    assert!(parsed["latest"].is_object());
    assert!(parsed["warnings"].is_array());
    assert!(parsed["routing"].is_object());
}

#[test]
fn tasks_status_all_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-status-all-json-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog.members]\ncatalog_a = \"catalog_a\"\n\n[tasks.check]\nrun = \"printf check\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf build\"\n",
    );
    seed_latest_task_status(&root, "catalog_a/build");

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            status_selector: None,
            status_all: true,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run task status all json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.tasks-status-all.v1");
    assert_eq!(
        parsed["scope_root"],
        fs::canonicalize(&root)
            .unwrap_or(root.clone())
            .display()
            .to_string()
    );
    assert!(parsed["catalog_scopes"].is_array());
    assert!(parsed["rows"].is_array());
    assert!(parsed["counts_by_state"].is_object());
    assert!(parsed["warnings"].is_array());
}

fn sorted_object_keys(value: &serde_json::Value) -> Vec<&str> {
    let mut keys = value
        .as_object()
        .expect("top-level object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<&str>>();
    keys.sort_unstable();
    keys
}

fn seed_latest_task_status(root: &Path, selector: &str) {
    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let selected_catalog_root = selector
        .split_once('/')
        .map(|(prefix, _)| canonical_root.join(prefix))
        .unwrap_or_else(|| canonical_root.clone());
    let identity = TaskStatusTargetIdentity::new(
        canonical_root.clone(),
        selected_catalog_root,
        selector,
        selector.rsplit('/').next().unwrap_or(selector),
        None,
    );
    let key = identity.status_key();
    let path = task_status_latest_record_path(root, &key);
    let record = TaskStatusCompletedRecord {
        status_key: key,
        identity,
        state: TaskStatusState::Succeeded,
        stage: Some(TaskStatusStage::Finishing),
        execution_surface: ExecutionSurface::DirectCli,
        runtime_route: TaskStatusRuntimeRouteSummary {
            route: "host".to_owned(),
            container: None,
            service: None,
        },
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
    fs::create_dir_all(path.parent().expect("latest parent")).expect("create status dir");
    fs::write(
        &path,
        serde_json::to_vec_pretty(&record).expect("encode latest status"),
    )
    .expect("write latest status");
}

fn timestamp_now() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}
