use super::prelude::*;

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
    assert!(parsed["findings"].is_array());
}

#[test]
fn doctor_explain_json_contract_has_selection_and_deferral_fields() {
    let root = temp_workspace("doctor-explain-json-contract");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "farmyard/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("run doctor explain json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.explain.v1");
    assert_eq!(parsed["request"]["task"], "farmyard/build");
    assert!(parsed["request"]["args"].is_array());
    assert_eq!(parsed["selection"]["status"], "ok");
    assert!(parsed["selection"]["evidence"].is_array());
    assert!(parsed["candidates"].is_array());
    assert!(parsed["deferral"]["considered"].is_boolean());
    assert!(parsed["deferral"]["selected"].is_boolean());
    assert!(parsed["reasoning"]["selection"].is_string());
    assert!(parsed["reasoning"]["deferral"].is_string());
}

#[test]
fn doctor_explain_json_snapshot_prefix_is_stable() {
    let root = temp_workspace("doctor-explain-json-snapshot");
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "farmyard/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("run doctor explain json");

    let parsed = parse_json(&out);
    let keys = parsed
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    assert_eq!(
        keys,
        vec![
            "ambiguity_candidates".to_owned(),
            "candidates".to_owned(),
            "deferral".to_owned(),
            "reasoning".to_owned(),
            "request".to_owned(),
            "root_resolution".to_owned(),
            "schema".to_owned(),
            "schema_version".to_owned(),
            "selection".to_owned(),
        ]
    );
    assert_schema_v1(&parsed, "effigy.doctor.explain.v1");
    assert_eq!(parsed["request"]["task"], "farmyard/build");
    assert_eq!(
        parsed["reasoning"]["selection"],
        "selected catalog by explicit task prefix"
    );
}
