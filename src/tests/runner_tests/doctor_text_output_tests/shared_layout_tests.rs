use super::prelude::*;

#[test]
fn run_doctor_and_explain_text_headings_are_stable() {
    let doctor_root = temp_workspace("doctor-shared-layout-heading");
    write_root_manifest(&doctor_root, "[tasks.health]\nrun = \"printf ok\"\n");
    let doctor_out = run_builtin_ok(doctor_root, "doctor", &[]);
    assert!(doctor_out.starts_with("Doctor's Report\n"));

    let explain_root =
        setup_doctor_explain_catalog_workspace("doctor-explain-shared-layout-heading");
    let explain_out = run_builtin_ok(explain_root, "doctor", &["farmyard/build", "--", "--watch"]);
    assert!(explain_out.starts_with("Doctor Explain\n"));
}

#[test]
fn run_doctor_and_explain_text_row_and_section_order_contract_is_stable() {
    let doctor_root = temp_workspace("doctor-shared-layout-row-order");
    write_root_manifest(
        &doctor_root,
        "[catalog]\nalias = \"root\"\nunknown_key = true\n",
    );
    let doctor_rendered = doctor_nonzero_rendered(run_doctor_err_from_cwd(&doctor_root, false));

    let evidence_idx = doctor_rendered
        .find("\nevidence:\n")
        .expect("doctor output should include evidence section");
    let remediation_idx = doctor_rendered
        .find("\nremediation:\n")
        .expect("doctor output should include remediation section");
    let auto_fix_idx = doctor_rendered
        .find("auto-fix:")
        .expect("doctor output should include auto-fix row");
    assert!(
        evidence_idx < remediation_idx && remediation_idx < auto_fix_idx,
        "doctor summary rows/sections must stay evidence -> remediation -> auto-fix ordered"
    );

    let explain_root = setup_doctor_explain_catalog_workspace("doctor-explain-shared-layout-order");
    let explain_rendered =
        run_builtin_ok(explain_root, "doctor", &["farmyard/build", "--", "--watch"]);
    let (prefix_block, _) = explain_rendered
        .split_once("\ncandidate-catalogs:\n")
        .expect("expected candidate-catalogs section");
    let ordered_keys = prefix_block
        .lines()
        .skip(2)
        .filter_map(|line| line.split_once(": ").map(|(key, _)| key.to_owned()))
        .collect::<Vec<String>>();
    assert_eq!(
        ordered_keys,
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
}
