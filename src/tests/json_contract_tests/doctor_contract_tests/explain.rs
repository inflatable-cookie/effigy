use super::*;

#[test]
fn doctor_explain_json_contract_has_selection_and_deferral_fields() {
    let root = temp_workspace("doctor-explain-json-contract");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf catalog_a\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "catalog_a/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("run doctor explain json");

    let parsed = parse_json(&out);
    assert_schema_v1(&parsed, "effigy.doctor.explain.v1");
    assert_eq!(parsed["request"]["task"], "catalog_a/build");
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
fn doctor_explain_text_and_json_reasoning_fields_and_order_are_consistent() {
    let root = temp_workspace("doctor-explain-text-json-parity");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf catalog_a\"\n",
    );

    let text = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec![
                "catalog_a/build".to_owned(),
                "--".to_owned(),
                "--watch".to_owned(),
            ],
        },
        root.clone(),
    )
    .expect("doctor explain text");

    let json = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "catalog_a/build".to_owned(),
                args: vec!["--".to_owned(), "--watch".to_owned()],
            }),
        })
    })
    .expect("doctor explain json");
    let parsed = parse_json(&json);

    let rows = parse_explain_prefix_rows(&text);
    let keys = rows
        .iter()
        .map(|(key, _)| key.clone())
        .collect::<Vec<String>>();
    assert_eq!(
        keys,
        vec![
            "request".to_owned(),
            "args".to_owned(),
            "resolved-root".to_owned(),
            "selection-status".to_owned(),
            "selected-catalog".to_owned(),
            "selected-mode".to_owned(),
            "selection-reasoning".to_owned(),
            "deferral-considered".to_owned(),
            "deferral-selected".to_owned(),
            "deferral-reasoning".to_owned(),
        ]
    );

    assert_eq!(
        explain_row_value(&rows, "selection-status"),
        parsed["selection"]["status"]
            .as_str()
            .expect("selection.status"),
    );
    assert_eq!(
        explain_row_value(&rows, "selected-catalog"),
        parsed["selection"]["catalog"]
            .as_str()
            .expect("selection.catalog"),
    );
    assert_eq!(
        explain_row_value(&rows, "selected-mode"),
        parsed["selection"]["mode"]
            .as_str()
            .expect("selection.mode"),
    );
    assert_eq!(
        explain_row_value(&rows, "selection-reasoning"),
        parsed["reasoning"]["selection"]
            .as_str()
            .expect("reasoning.selection"),
    );
    assert_eq!(
        explain_row_value(&rows, "deferral-considered"),
        &parsed["deferral"]["considered"]
            .as_bool()
            .expect("deferral.considered")
            .to_string(),
    );
    assert_eq!(
        explain_row_value(&rows, "deferral-selected"),
        &parsed["deferral"]["selected"]
            .as_bool()
            .expect("deferral.selected")
            .to_string(),
    );
    assert_eq!(
        explain_row_value(&rows, "deferral-reasoning"),
        parsed["reasoning"]["deferral"]
            .as_str()
            .expect("reasoning.deferral"),
    );

    let reasoning_keys = parsed["reasoning"]
        .as_object()
        .expect("reasoning object")
        .keys()
        .cloned()
        .collect::<Vec<String>>();
    assert_eq!(
        reasoning_keys,
        vec!["deferral".to_owned(), "selection".to_owned()]
    );
}

#[test]
fn doctor_explain_json_snapshot_prefix_is_stable() {
    let root = temp_workspace("doctor-explain-json-snapshot");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.root]\nrun = \"printf root\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.build]\nrun = \"printf catalog_a\"\n",
    );

    let out = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: true,
            fix: false,
            verbose: false,
            explain: Some(TaskInvocation {
                name: "catalog_a/build".to_owned(),
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
    assert_eq!(parsed["request"]["task"], "catalog_a/build");
    assert_eq!(
        parsed["reasoning"]["selection"],
        "selected catalog by explicit task prefix"
    );
}
