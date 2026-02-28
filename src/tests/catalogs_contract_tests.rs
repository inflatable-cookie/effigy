use super::{run_manifest_task_with_cwd, TaskInvocation};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn catalogs_text_contract_includes_core_sections_and_probe_fields() {
    let root = temp_workspace("catalogs-contract-text");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--resolve".to_owned(), "farmyard/api".to_owned()],
        },
        root,
    )
    .expect("run catalogs text");

    let expected_markers = [
        "Resolution: farmyard/api",
        "catalog: farmyard",
        "task: api",
        "evidence:",
        "selected catalog via explicit prefix `farmyard`",
    ];
    for marker in expected_markers {
        assert!(
            out.contains(marker),
            "catalogs text output missing marker `{marker}`"
        );
    }
}

#[test]
fn catalogs_json_pretty_contract_uses_tasks_schema_top_level_shape() {
    let root = temp_workspace("catalogs-contract-json-pretty");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("run catalogs pretty json");

    assert!(out.contains('\n'));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
    assert!(parsed["catalogs"].is_array());
    assert_eq!(parsed["resolve"]["catalog"], "farmyard");
}

#[test]
fn catalogs_json_compact_contract_is_single_line_and_valid_json() {
    let root = temp_workspace("catalogs-contract-json-compact");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");

    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "false".to_owned(),
                "--resolve".to_owned(),
                "farmyard/api".to_owned(),
            ],
        },
        root,
    )
    .expect("run catalogs compact json");

    assert!(!out.contains('\n'));
    assert!(!out.contains('\n'));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["resolve"]["status"], "ok");
}

#[test]
fn catalogs_json_contract_reports_builtin_resolve_as_ok() {
    let root = temp_workspace("catalogs-contract-json-builtin-resolve");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "test".to_owned(),
            ],
        },
        root,
    )
    .expect("run catalogs builtin resolve json");

    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["task"], "test");
    assert!(parsed["resolve"]["catalog"].is_null());
    assert!(parsed["resolve"]["error"].is_null());
    assert!(parsed["resolve"]["evidence"]
        .as_array()
        .is_some_and(|items| items
            .iter()
            .filter_map(|item| item.as_str())
            .any(|item| item.contains("resolved built-in task `test`"))));
}

fn write_manifest(path: &PathBuf, body: &str) {
    fs::write(path, body).expect("write manifest");
}

fn temp_dir(name: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("effigy-catalogs-contract-{name}-{ts}"))
}

fn temp_workspace(name: &str) -> PathBuf {
    let root = temp_dir(name);
    fs::create_dir_all(&root).expect("mkdir workspace");
    fs::write(root.join("package.json"), "{}\n").expect("write package marker");
    root
}
