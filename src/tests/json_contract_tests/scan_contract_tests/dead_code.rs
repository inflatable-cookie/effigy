use super::*;

#[test]
fn builtin_scan_dead_code_json_contract_reports_advisory_findings() {
    let root = temp_workspace("scan-dead-code-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        r#"
[scan.dead_code]
doctor = false
allow_paths = ["src/bin/**"]
"#,
    );
    fs::create_dir_all(root.join("src/bin")).expect("mkdir bin");
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
        "pub fn lonely() -> usize { 1 }\npub fn helper() -> usize { 2 }\n",
    )
    .expect("write orphan");
    fs::write(
        root.join("src/bin/tool.rs"),
        "pub fn main() -> usize { 1 }\n",
    )
    .expect("write bin");
    effigy_codegraph::run_index(&root).expect("graph index");

    let parsed = run_invocation_json(root, "scan", &["dead-code", "--json"]);
    assert_schema_v1(&parsed, "effigy.scan.dead-code.v1");
    assert_eq!(parsed["scan"], "dead-code");
    assert!(parsed["checked_files"].as_u64().is_some());
    assert!(parsed["checked_symbols"].as_u64().is_some());
    let findings = parsed["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty());
    let isolated = findings
        .iter()
        .find(|finding| finding["kind"] == "isolated-file")
        .expect("isolated file finding");
    assert_eq!(isolated["path"], "src/orphan/mod.rs");
    assert_eq!(isolated["confidence"], "high");
    assert!(isolated["reason"].is_string());
}
