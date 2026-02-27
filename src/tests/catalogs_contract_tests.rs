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
        "Catalog Diagnostics",
        "catalogs: 2",
        "Routing Precedence",
        "1) explicit catalog alias prefix",
        "2) relative/absolute catalog path prefix",
        "3) unprefixed nearest in-scope catalog by cwd",
        "4) unprefixed shallowest catalog from workspace root",
        "Resolution Probe: farmyard/api",
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
fn catalogs_json_pretty_contract_uses_expected_top_level_shape() {
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

    assert!(out.starts_with("{\n  \"catalogs\": ["));
    assert!(out.contains("\n  \"precedence\": ["));
    assert!(out.contains("\n  \"resolve\": {"));
    assert!(out.contains("\n    \"selector\": \"farmyard/api\""));
    assert!(out.contains("\n    \"status\": \"ok\""));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
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
    assert!(out.starts_with("{\"catalogs\":["));
    assert!(out.contains("\"precedence\":["));
    assert!(out.contains("\"resolve\":{"));
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("parse json");
    assert_eq!(parsed["resolve"]["status"], "ok");
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
