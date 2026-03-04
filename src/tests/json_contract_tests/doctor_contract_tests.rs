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
fn doctor_explain_text_and_json_reasoning_fields_and_order_are_consistent() {
    let root = temp_workspace("doctor-explain-text-json-parity");
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

    let text = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec![
                "farmyard/build".to_owned(),
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
                name: "farmyard/build".to_owned(),
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

fn run_doctor_rendered(root: PathBuf, output_json: bool) -> String {
    match run_doctor(DoctorArgs {
        repo_override: Some(root),
        output_json,
        fix: false,
        verbose: false,
        explain: None,
    }) {
        Ok(rendered) => rendered,
        Err(RunnerError::DoctorNonZero { rendered, .. }) => rendered,
        Err(other) => panic!("unexpected doctor error: {other}"),
    }
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

fn parse_explain_prefix_rows(rendered: &str) -> Vec<(String, String)> {
    let (prefix_block, _) = rendered
        .split_once("\ncandidate-catalogs:\n")
        .expect("expected candidate-catalogs section");
    prefix_block
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (key, value) = line.split_once(": ")?;
            Some((key.to_owned(), value.to_owned()))
        })
        .collect::<Vec<(String, String)>>()
}

fn explain_row_value<'a>(rows: &'a [(String, String)], key: &str) -> &'a str {
    rows.iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
        .unwrap_or_else(|| panic!("missing explain row `{key}`"))
}
