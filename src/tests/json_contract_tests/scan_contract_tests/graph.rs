use super::*;

#[test]
fn builtin_scan_boundary_violations_json_contract_reports_precise_findings() {
    let root = temp_workspace("scan-boundary-violations-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[scan.boundary_violations]
doctor = false

[scan.boundary_violations.layers.app]
paths = ["src/app/**"]
may_depend_on = ["domain", "shared"]

[scan.boundary_violations.layers.domain]
paths = ["src/domain/**"]
may_depend_on = ["shared"]

[scan.boundary_violations.layers.shared]
paths = ["src/shared/**"]
"#,
    );
    fs::create_dir_all(root.join("src/app")).expect("mkdir app");
    fs::create_dir_all(root.join("src/domain")).expect("mkdir domain");
    fs::create_dir_all(root.join("src/shared")).expect("mkdir shared");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod app;\npub mod domain;\npub mod shared;\n",
    )
    .expect("write lib");
    fs::write(root.join("src/shared/mod.rs"), "pub fn shared() {}\n").expect("write shared");
    fs::write(root.join("src/app/mod.rs"), "pub fn page() {}\n").expect("write app");
    fs::write(
        root.join("src/domain/mod.rs"),
        "use crate::app::page;\nuse crate::shared::shared;\npub fn service() { page(); shared(); }\n",
    )
    .expect("write domain");
    effigy_codegraph::run_index(&root).expect("graph index");

    let parsed = run_invocation_json(root, "scan", &["boundary-violations", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.boundary-violations.v1");
    assert_eq!(parsed["scan"], "boundary-violations");
    assert_eq!(parsed["configured_layers"], 3);
    assert!(parsed["finding_count"]
        .as_u64()
        .is_some_and(|count| count >= 1));
    let findings = parsed["findings"].as_array().expect("findings array");
    let finding = findings
        .iter()
        .find(|finding| {
            finding["source_layer"] == "domain"
                && finding["target_layer"] == "app"
                && finding["source_path"] == "src/domain/mod.rs"
                && finding["target_path"] == "src/app/mod.rs"
        })
        .expect("domain->app finding");
    assert!(finding["edge_kind"].is_string());
    assert!(finding["source_line"].is_number());
    assert!(finding["target_line"].is_number());
}
