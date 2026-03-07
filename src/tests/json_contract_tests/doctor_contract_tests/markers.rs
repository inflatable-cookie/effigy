use super::*;

#[test]
fn doctor_json_contract_includes_scan_attention_markers_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-attention-markers");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.attention_markers]
warning = ["TODO"]
high = ["FIXME"]
critical = ["BLOCKER"]
"#,
    );
    fs::write(
        root.join("src/app.ts"),
        "// TODO: revisit\n// FIXME: remove workaround before merge\n",
    )
    .expect("write source");

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    assert_scan_findings(
        &parsed,
        "scan.attention-markers",
        "error",
        &[
            ("warning", "[warning] deferred-work [TODO]"),
            ("error", "[high] deferred-work [FIXME]"),
        ],
    );
}

#[test]
fn doctor_json_contract_omits_scan_attention_markers_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-attention-markers-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.attention_markers]
warning = ["TODO"]
high = ["FIXME"]
critical = ["BLOCKER"]
doctor = false
"#,
    );
    fs::write(
        root.join("src/app.ts"),
        "// FIXME: remove workaround before merge\n",
    )
    .expect("write source");

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.attention-markers");
}

#[test]
fn doctor_json_contract_reports_scan_stale_suppressions_findings() {
    let root = temp_workspace("doctor-json-scan-stale-suppressions");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r##"[scan.stale_suppressions]
warning = ["eslint-disable-next-line"]
high = ["#[allow("]
critical = ["eslint-disable"]
doctor = true
"##,
    );
    fs::write(
        root.join("src/app.ts"),
        "// eslint-disable-next-line no-console\n// eslint-disable\n",
    )
    .expect("write source");

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    assert_scan_findings(
        &parsed,
        "scan.stale-suppressions",
        "error",
        &[
            (
                "warning",
                "[warning] lint-disable [eslint-disable-next-line]",
            ),
            ("error", "[critical] lint-disable [eslint-disable]"),
        ],
    );
}

#[test]
fn doctor_json_contract_omits_scan_stale_suppressions_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-stale-suppressions-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r##"[scan.stale_suppressions]
warning = ["eslint-disable-next-line"]
high = ["#[allow("]
critical = ["eslint-disable"]
doctor = false
"##,
    );
    fs::write(root.join("src/app.ts"), "// eslint-disable\n").expect("write source");

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.stale-suppressions");
}
