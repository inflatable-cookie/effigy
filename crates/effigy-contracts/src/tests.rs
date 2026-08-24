use super::{resolve_schema_payload, split_shell_like_args, validate_selection, SelectionPayload};
use serde_json::json;
use std::path::Path;

#[test]
fn validate_selection_accepts_valid_payload() {
    let repo = tempfile::tempdir().expect("temp repo");
    let repo_root = repo.path();
    let contract_path = repo_root.join("contract.json");
    let artifact_path = repo_root.join("artifact.json");
    std::fs::write(
        &contract_path,
        json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full", "changed-only"]
                }
            }
        })
        .to_string(),
    )
    .expect("contract");
    std::fs::write(
        &artifact_path,
        json!({
            "selected": ["a", "b"],
            "count": 2,
            "changed_only_base": null,
            "mode": "full"
        })
        .to_string(),
    )
    .expect("artifact");

    let report = validate_selection(
        repo_root,
        Some(&Path::new("contract.json").to_path_buf()),
        Some(&Path::new("artifact.json").to_path_buf()),
    )
    .expect("selection validation");
    assert!(report.ok());
}

#[test]
fn validate_selection_rejects_wrong_count() {
    let repo = tempfile::tempdir().expect("temp repo");
    let repo_root = repo.path();
    let contract_path = repo_root.join("contract.json");
    let artifact_path = repo_root.join("artifact.json");
    std::fs::write(
        &contract_path,
        json!({
            "required": ["selected", "count", "changed_only_base", "mode"],
            "properties": {
                "mode": {
                    "enum": ["full"]
                }
            }
        })
        .to_string(),
    )
    .expect("contract");
    std::fs::write(
        &artifact_path,
        json!({
            "selected": ["a", "b"],
            "count": 1,
            "changed_only_base": null,
            "mode": "full"
        })
        .to_string(),
    )
    .expect("artifact");

    let report = validate_selection(
        repo_root,
        Some(&Path::new("contract.json").to_path_buf()),
        Some(&Path::new("artifact.json").to_path_buf()),
    )
    .expect("selection validation");
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("`count` must equal the number of `selected` entries")));
}

#[test]
fn render_selection_payload_json_starts_with_selected_key() {
    let payload = SelectionPayload {
        selected: vec!["a".to_owned(), "b".to_owned()],
        changed_only_base: None,
        mode: "full".to_owned(),
    };

    let rendered = payload.render_json().expect("render selection payload");
    assert!(rendered.starts_with("{\"selected\":"));
    assert_eq!(
        rendered,
        "{\"selected\":[\"a\",\"b\"],\"count\":2,\"changed_only_base\":null,\"mode\":\"full\"}"
    );
}

#[test]
fn split_shell_like_args_preserves_quoted_groups() {
    let args =
        split_shell_like_args("effigy --json doctor --repo \"/tmp/demo repo\" build -- --watch")
            .expect("args");
    assert_eq!(
        args,
        vec![
            "effigy",
            "--json",
            "doctor",
            "--repo",
            "/tmp/demo repo",
            "build",
            "--",
            "--watch",
        ]
    );
}

#[test]
fn resolve_schema_payload_prefers_nested_result_schema() {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "command": {"kind": "contracts", "name": "contracts"},
        "result": {
            "schema": "effigy.contracts.selection-validation.v1",
            "schema_version": 1,
            "ok": true
        },
        "error": null
    });

    let resolved = resolve_schema_payload("effigy.contracts.selection-validation.v1", &payload);
    assert_eq!(resolved.schema, "effigy.contracts.selection-validation.v1");
    assert_eq!(resolved.schema_version, "1");
    assert_eq!(resolved.payload["ok"], true);
}
