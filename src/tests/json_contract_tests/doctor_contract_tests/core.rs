use super::*;

#[test]
fn doctor_json_contract_has_versioned_top_level_shape() {
    let root = temp_workspace("doctor-json-contract");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.ok]\nrun = \"printf ok\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    })
    .expect("run doctor json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["summary"].is_object());
    assert!(parsed["sections"].is_array());
    assert!(parsed["findings"].is_array());
    assert!(parsed["fixes"].is_array());
    assert!(parsed["root_resolution"].is_object());
}

#[test]
fn doctor_json_contract_with_health_stdout_remains_valid_json() {
    let root = temp_workspace("doctor-json-contract-health-stdout");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf healthy\"\n",
    );

    let out = run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json: true,
        fix: false,
        verbose: false,
        explain: None,
    })
    .expect("run doctor json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    assert_eq!(parsed["ok"], true);
    assert!(parsed["sections"].is_array());
    assert!(parsed["findings"].is_array());
}

#[test]
fn doctor_json_sections_order_matches_text_group_render_order() {
    let root = temp_workspace("doctor-json-sections-order-parity");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\nunknown_key = true\n",
    );

    let json_rendered = run_doctor_rendered(root.clone(), true);
    let parsed = parse_json(&json_rendered);
    assert_schema_v1(&parsed, "effigy.doctor.v1");
    let section_ids = parsed["sections"]
        .as_array()
        .expect("sections array")
        .iter()
        .filter_map(|section| section["check_id"].as_str())
        .map(str::to_owned)
        .collect::<Vec<String>>();
    assert!(
        !section_ids.is_empty(),
        "expected at least one section in doctor json output"
    );

    let text_rendered = run_doctor_rendered(root, false);
    let mut last_index = 0usize;
    for (index, check_id) in section_ids.iter().enumerate() {
        let found = text_rendered
            .find(check_id)
            .unwrap_or_else(|| panic!("missing section `{check_id}` in text output"));
        if index > 0 {
            assert!(
                found > last_index,
                "text group order diverged from json sections order at `{check_id}`"
            );
        }
        last_index = found;
    }
}
