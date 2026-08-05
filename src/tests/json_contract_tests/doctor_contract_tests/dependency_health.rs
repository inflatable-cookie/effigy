use super::*;

#[test]
fn doctor_json_contract_surfaces_orphan_dependency_link_state() {
    let root = temp_workspace("doctor-json-dependency-link-health");
    write_manifest(&root.join("effigy.toml"), "");
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&cargo_dir).expect("mkdir .cargo");
    fs::write(
        cargo_dir.join("config.toml"),
        "# >>> effigy deps cargo /tmp/orphan-library >>>\n# <<< effigy deps cargo /tmp/orphan-library <<<\n",
    )
    .expect("write orphan managed block");

    let rendered = run_doctor_rendered(root.clone(), true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    let section = find_section(&parsed, "dependencies.link-health");
    assert_eq!(section["severity"], "error");
    assert_eq!(section["findings"].as_array().map(Vec::len), Some(1));
    let finding = &section["findings"][0];
    assert_eq!(finding["severity"], "error");
    assert!(finding["evidence"]
        .as_str()
        .is_some_and(|evidence| evidence.contains("reason=cargo-managed-block-without-ledger")));
    assert!(finding["evidence"]
        .as_str()
        .is_some_and(|evidence| evidence.contains("mechanism=cargo-patch")));

    let text = run_doctor_rendered(root, false);
    assert!(text.contains("dependencies.link-health"));
    assert!(text.contains("reason=cargo-managed-block-without-ledger"));
}
