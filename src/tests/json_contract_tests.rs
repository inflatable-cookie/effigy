use super::{
    run_doctor, run_manifest_task_with_cwd, run_tasks, DoctorArgs, RunnerError, TasksArgs,
};
use crate::TaskInvocation;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn tasks_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("tasks-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.dev]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: None,
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
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
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.filtered.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["matches"].is_array());
    assert!(parsed["managed_profile_matches"].is_array());
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["notes"].is_array());
}

#[test]
fn tasks_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-json-contract-resolve");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: None,
            resolve_selector: Some("farmyard/api".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run tasks json resolve");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "api");
}

#[test]
fn tasks_filtered_json_contract_with_resolve_has_diagnostics_and_probe_fields() {
    let root = temp_workspace("tasks-filtered-json-contract-resolve");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf build\"\n",
    );

    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("build".to_owned()),
            resolve_selector: Some("farmyard/build".to_owned()),
            output_json: true,
            pretty_json: true,
        })
    })
    .expect("run filtered tasks json resolve");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.filtered.v1");
    assert_eq!(parsed["filter"], "build");
    assert!(parsed["catalogs"].is_array());
    assert!(parsed["precedence"].is_array());
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
    assert_eq!(parsed["resolve"]["task"], "build");
}

#[test]
fn doctor_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("doctor-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ok]\nrun = \"printf ok\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
    })
    .expect("run doctor json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.doctor.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert!(parsed["summary"].is_object());
    assert!(parsed["findings"].is_array());
    assert!(parsed["fixes"].is_array());
    assert!(parsed["root_resolution"].is_object());
}

#[test]
fn doctor_json_contract_with_health_stdout_remains_valid_json() {
    let root = temp_workspace("doctor-json-contract-health-stdout");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf healthy\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
    })
    .expect("run doctor json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.doctor.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert!(parsed["findings"].is_array());
}

#[test]
fn builtin_test_plan_json_contract_has_versioned_shape_and_suite_source_fields() {
    let root = temp_workspace("test-plan-json-contract");
    fs::write(
        root.join("package.json"),
        r#"{
  "devDependencies": {
    "vitest": "^2.0.0"
  }
}"#,
    )
    .expect("write package");

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec!["--plan".to_owned(), "--json".to_owned()],
        },
        root,
    )
    .expect("run test --plan --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.test.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["targets"].is_array());
    let first = parsed["targets"]
        .as_array()
        .and_then(|targets| targets.first())
        .expect("target entry");
    assert!(first["suite_source"].is_string());
    assert!(first["available_suites"].is_array());
    assert!(first["fallback_chain"].is_array());
}

#[test]
fn builtin_test_results_json_contract_has_versioned_shape_and_hint_fields() {
    let root = temp_workspace("test-results-json-contract");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"test\": \"vitest\" } }\n",
    )
    .expect("write package");
    let local_bin = root.join("node_modules/.bin");
    fs::create_dir_all(&local_bin).expect("mkdir local bin");
    let vitest = local_bin.join("vitest");
    fs::write(&vitest, "#!/bin/sh\nexit 1\n").expect("write vitest");
    let mut perms = fs::metadata(&vitest).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&vitest, perms).expect("chmod");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "test".to_owned(),
            args: vec![
                "--json".to_owned(),
                "vitest".to_owned(),
                "user-service".to_owned(),
            ],
        },
        root,
    )
    .expect_err("expected non-zero test failure");

    let rendered = match err {
        RunnerError::BuiltinTestNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.test.results.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["targets"].is_array());
    assert!(parsed["failures"].is_array());
    assert!(parsed["hint"].is_object());
    assert_eq!(
        parsed["hint"]["kind"],
        serde_json::Value::String("selected-suite-filter-no-match".to_owned())
    );
}

#[test]
fn builtin_help_json_contract_has_versioned_shape() {
    let root = temp_workspace("help-json-contract");
    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "help".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect("run help --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.help.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["topic"], "general");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("Commands")));
}

#[test]
fn builtin_config_json_contract_has_versioned_shape() {
    let root = temp_workspace("config-json-contract");
    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "config".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect("run config --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.config.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["mode"], "reference");
    assert!(parsed["text"]
        .as_str()
        .is_some_and(|text| text.contains("effigy.toml Reference")));
}

#[test]
fn catalog_task_run_json_contract_success_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-success");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "build".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect("run build --json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["exit_code"], 0);
    assert_eq!(parsed["stdout"], "build-ok");
}

#[test]
fn catalog_task_run_json_contract_failure_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "fail".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect_err("expected non-zero task failure");

    let rendered = match err {
        RunnerError::CommandJsonFailure { rendered } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["task"], "fail");
    assert_eq!(parsed["exit_code"], 9);
    assert_eq!(parsed["stdout"], "fail-out");
    assert_eq!(parsed["stderr"], "fail-err");
}

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-json-contract-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}

fn with_cwd<F, T>(cwd: &PathBuf, f: F) -> T
where
    F: FnOnce() -> T,
{
    let _guard = test_lock().lock().expect("lock");
    let original = std::env::current_dir().expect("current dir");
    std::env::set_current_dir(cwd).expect("set cwd");
    let out = f();
    std::env::set_current_dir(original).expect("restore cwd");
    out
}

fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
