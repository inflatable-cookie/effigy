use super::test_support::execution::run_manifest_task_with_cwd;
use crate::contract_test_support::{parse_json, temp_workspace, write_manifest};
use crate::TaskInvocation;
use std::fs;

#[test]
fn catalogs_text_contract_includes_core_sections_and_probe_fields() {
    let root = temp_workspace("catalogs-contract-text");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");

    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec!["--resolve".to_owned(), "catalog_a/api".to_owned()],
        },
        root,
    )
    .expect("run catalogs text");

    let expected_markers = [
        "Resolution: catalog_a/api",
        "catalog: catalog_a",
        "task: api",
        "lock_scopes: workspace, task:api",
        "evidence:",
        "selected catalog via explicit prefix `catalog_a`",
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
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");

    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--resolve".to_owned(),
                "catalog_a/api".to_owned(),
            ],
        },
        root,
    )
    .expect("run catalogs pretty json");

    assert!(out.contains('\n'));
    let parsed = parse_json(&out);
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["catalog_tasks"].is_array());
    assert!(parsed["builtin_tasks"].is_array());
    assert!(parsed["catalogs"].is_array());
    assert_eq!(parsed["resolve"]["catalog"], "catalog_a");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "workspace");
    assert_eq!(parsed["resolve"]["lock_scopes"][1], "task:api");
}

#[test]
fn catalogs_json_compact_contract_is_single_line_and_valid_json() {
    let root = temp_workspace("catalogs-contract-json-compact");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");

    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.api]\nrun = \"printf api\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "catalogs".to_owned(),
            args: vec![
                "--json".to_owned(),
                "--pretty".to_owned(),
                "false".to_owned(),
                "--resolve".to_owned(),
                "catalog_a/api".to_owned(),
            ],
        },
        root,
    )
    .expect("run catalogs compact json");

    assert!(!out.contains('\n'));
    assert!(!out.contains('\n'));
    let parsed = parse_json(&out);
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["lock_scopes"][0], "workspace");
    assert_eq!(parsed["resolve"]["lock_scopes"][1], "task:api");
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

    let parsed = parse_json(&out);
    assert_eq!(parsed["schema"], "effigy.tasks.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["resolve"]["status"], "ok");
    assert_eq!(parsed["resolve"]["task"], "test");
    assert!(parsed["resolve"]["catalog"].is_null());
    assert!(parsed["resolve"]["lock_scopes"]
        .as_array()
        .is_some_and(|items| items.is_empty()));
    assert!(parsed["resolve"]["error"].is_null());
    assert!(parsed["resolve"]["evidence"]
        .as_array()
        .is_some_and(|items| items
            .iter()
            .filter_map(|item| item.as_str())
            .any(|item| item.contains("resolved built-in task `test`"))));
}
