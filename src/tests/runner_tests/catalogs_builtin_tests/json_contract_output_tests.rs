use super::prelude::*;

#[test]
fn run_manifest_task_builtin_catalogs_json_renders_probe_payload() {
    let root = temp_workspace("builtin-catalogs-json");
    write_root_and_farmyard_api_catalog(&root);

    let out = run_catalogs_ok(root, &["--json", "--resolve", "farmyard/api"]);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    assert_json_array_field(&parsed, "catalogs");
    assert_json_string_field_eq(&parsed["resolve"], "status", "ok");
    assert_json_string_field_eq(&parsed["resolve"], "catalog", "farmyard");
    assert_json_string_field_eq(&parsed["resolve"], "task", "api");
    assert_json_array_field(&parsed, "precedence");
}

#[test]
fn run_manifest_task_builtin_catalogs_json_resolve_supports_managed_profile_invocation() {
    let root = temp_workspace("builtin-catalogs-json-resolve-managed-profile");
    write_managed_dev_profile_manifest(&root, "front");

    let out = run_catalogs_ok(root, &["--json", "--resolve", "dev front"]);

    let parsed = parse_json_output_with_schema(&out, "effigy.tasks.v1");
    assert_json_string_field_eq(&parsed["resolve"], "selector", "dev front");
    assert_json_string_field_eq(&parsed["resolve"], "status", "ok");
    assert_json_string_field_eq(&parsed["resolve"], "catalog", "root");
    assert_json_string_field_eq(&parsed["resolve"], "task", "dev");
    let evidence = parsed["resolve"]["evidence"]
        .as_array()
        .expect("resolve evidence array")
        .iter()
        .filter_map(|line| line.as_str())
        .collect::<Vec<&str>>();
    assert!(evidence
        .iter()
        .any(|line| line.contains("managed profile `front` resolved via invocation `dev front`")));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_reports_resolution_errors() {
    let root = temp_workspace("builtin-catalogs-json-error");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");

    let out = run_catalogs_ok(root, &["--json", "--resolve", "farmyard/api"]);

    let parsed = parse_json_output_with_schema_version(&out, "effigy.tasks.v1", 1);
    assert_json_string_field_eq(&parsed["resolve"], "status", "error");
    assert_eq!(parsed["resolve"]["catalog"], serde_json::Value::Null);
    assert!(parsed["resolve"]["error"]
        .as_str()
        .is_some_and(|msg| msg.contains("prefix `farmyard` not found")));
}

#[test]
fn run_manifest_task_builtin_catalogs_json_compact_output_has_no_newlines() {
    let root = temp_workspace("builtin-catalogs-json-compact");
    write_root_and_farmyard_api_catalog(&root);

    let out = run_catalogs_ok(
        root,
        &["--json", "--pretty", "false", "--resolve", "farmyard/api"],
    );

    assert_output_excludes_all(&out, &["\n"]);
    let parsed = parse_json_output_with_schema(&out, "effigy.tasks.v1");
    assert_json_string_field_eq(&parsed["resolve"], "status", "ok");
}
