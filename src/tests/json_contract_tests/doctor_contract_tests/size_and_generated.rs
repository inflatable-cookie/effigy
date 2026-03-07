use super::*;

#[test]
fn doctor_json_contract_includes_scan_god_files_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-god-files");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 14
critical = 20
"#,
    );
    let warning_file = (0..12)
        .map(|idx| format!("const warning_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    let high_file = (0..15)
        .map(|idx| format!("const high_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/warning.ts"), format!("{warning_file}\n")).expect("write warning");
    fs::write(root.join("src/high.ts"), format!("{high_file}\n")).expect("write high");

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    assert_scan_findings(
        &parsed,
        "scan.god-files",
        "error",
        &[
            ("warning", "[warning] src/warning.ts"),
            ("error", "[high] src/high.ts"),
        ],
    );
}

#[test]
fn doctor_json_contract_omits_scan_god_files_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-god-files-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 14
critical = 20
doctor = false
"#,
    );
    let body = (0..15)
        .map(|idx| format!("const high_{idx} = {idx};"))
        .collect::<Vec<String>>()
        .join("\n");
    fs::write(root.join("src/high.ts"), format!("{body}\n")).expect("write high");

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.god-files");
}

#[test]
fn doctor_json_contract_includes_scan_generated_assets_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-generated-assets");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_assets]
warn = 100
high = 150
critical = 300
"#,
    );
    fs::write(root.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    assert_scan_findings(
        &parsed,
        "scan.generated-assets",
        "error",
        &[("error", "[high] dist/app.min.js")],
    );
}

#[test]
fn doctor_json_contract_omits_scan_generated_assets_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-generated-assets-disabled");
    fs::create_dir_all(root.join("dist")).expect("mkdir dist");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_assets]
warn = 100
high = 150
critical = 300
doctor = false
"#,
    );
    fs::write(root.join("dist/app.min.js"), vec![b'a'; 180]).expect("write asset");

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.generated-assets");
}

#[test]
fn doctor_json_contract_includes_scan_generated_in_src_sections_and_findings() {
    let root = temp_workspace("doctor-json-scan-generated-in-src");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_in_src]
warn = 100
high = 150
critical = 300
"#,
    );
    fs::write(root.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

    let rendered = run_doctor_rendered(root, true);
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], false);

    assert_scan_findings(
        &parsed,
        "scan.generated-in-src",
        "error",
        &[("error", "[high] src/client.generated.ts")],
    );
}

#[test]
fn doctor_json_contract_omits_scan_generated_in_src_when_doctor_flag_is_disabled() {
    let root = temp_workspace("doctor-json-scan-generated-in-src-disabled");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.generated_in_src]
warn = 100
high = 150
critical = 300
doctor = false
"#,
    );
    fs::write(root.join("src/client.generated.ts"), vec![b'a'; 180]).expect("write asset");

    let parsed = run_doctor_json(root);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert_scan_omitted(&parsed, "scan.generated-in-src");
}
