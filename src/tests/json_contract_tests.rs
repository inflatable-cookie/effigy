use super::{run_pulse, run_tasks, PulseArgs, TasksArgs};
use std::fs;
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
            output_json: true,
        })
    })
    .expect("run tasks json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
}

#[test]
fn tasks_filtered_json_contract_has_versioned_shape_and_filter_fields() {
    let root = temp_workspace("tasks-filtered-json-contract");
    let out = with_cwd(&root, || {
        run_tasks(TasksArgs {
            repo_override: None,
            task_name: Some("test".to_owned()),
            output_json: true,
        })
    })
    .expect("run filtered tasks json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.filtered.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["filter"], "test");
    assert!(parsed["matches"].is_array());
    assert!(parsed["builtin_matches"].is_array());
    assert!(parsed["notes"].is_array());
}

#[test]
fn repo_pulse_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("repo-pulse-json-contract");
    let out = run_pulse(PulseArgs {
        repo_override: Some(root),
        verbose_root: false,
        output_json: true,
    })
    .expect("run repo-pulse json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.repo-pulse.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["report"].is_object());
    assert!(parsed["root_resolution"].is_object());
    assert!(parsed["report"]["evidence"].is_array());
    assert!(parsed["report"]["risk"].is_array());
    assert!(parsed["report"]["next_action"].is_array());
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
