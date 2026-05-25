use super::*;

#[test]
fn builtin_scan_validation_gaps_json_contract_reports_changed_owner_findings() {
    let root = temp_workspace("scan-validation-gaps-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[scan.validation_gaps]
doctor = false
hotspot_threshold = 1
"#,
    );
    fs::create_dir_all(root.join("src/live")).expect("mkdir live");
    fs::create_dir_all(root.join("src/orphan")).expect("mkdir orphan");
    fs::write(root.join("src/lib.rs"), "pub mod live;\npub mod orphan;\n").expect("write lib");
    fs::write(
        root.join("src/live/mod.rs"),
        "use crate::orphan::helper;\npub fn used() -> usize { helper() }\n",
    )
    .expect("write live");
    fs::write(
        root.join("src/orphan/mod.rs"),
        "pub fn helper() -> usize { 2 }\n",
    )
    .expect("write orphan");
    effigy_codegraph::run_index(&root).expect("graph index");

    let parsed = run_invocation_json(
        root,
        "scan",
        &["validation-gaps", "--path", "src/orphan/mod.rs", "--json"],
    );
    assert_schema_v1(&parsed, "effigy.scan.validation-gaps.v1");
    assert_eq!(parsed["scan"], "validation-gaps");
    assert_eq!(parsed["mode"], "changed-paths");
    assert_eq!(parsed["hotspot_threshold"], 1);
    assert_eq!(parsed["changed_paths"][0], "src/orphan/mod.rs");
    let findings = parsed["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty());
    let changed = findings
        .iter()
        .find(|finding| finding["kind"] == "changed-owner-without-test-target")
        .expect("changed owner finding");
    assert_eq!(changed["path"], "src/orphan/mod.rs");
    assert_eq!(changed["confidence"], "high");
}
