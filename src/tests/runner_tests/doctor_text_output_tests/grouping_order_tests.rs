use super::prelude::*;

#[test]
fn run_doctor_verbose_text_output_includes_per_finding_entries() {
    let root = temp_workspace("doctor-verbose-entries");
    write_root_manifest(
        &root,
        r#"[tasks.alpha]
run = [{ task = "missing/task" }]

[tasks.beta]
run = [{ task = "missing/task" }]
"#,
    );

    let err = run_builtin_err(root, "doctor", &["--verbose"]);
    let rendered = doctor_nonzero_rendered(err);

    assert_output_contains_all(
        &rendered,
        &[
            "tasks.references.resolve",
            "findings: 2",
            "entry: 1",
            "entry: 2",
            "entry-evidence",
            "entry-remediation",
        ],
    );
}

#[test]
fn run_doctor_groups_findings_in_severity_first_order() {
    let root = temp_workspace("doctor-severity-order");
    write_root_manifest(&root, "[catalog]\nalias = \"root\"\nunknown_key = true\n");

    let err = run_doctor_err_from_cwd(&root, false);

    let rendered = doctor_nonzero_rendered(err);

    let error_idx = rendered
        .find("manifest.parse")
        .or_else(|| rendered.find("manifest.schema.unsupported_key"))
        .expect("expected error finding");
    let warning_idx = rendered
        .find("health.task.discovery")
        .expect("expected warning finding");
    let info_idx = rendered
        .find("workspace.root-resolution")
        .expect("expected info finding");

    assert!(
        error_idx < warning_idx,
        "error should be rendered before warning"
    );
    assert!(
        warning_idx < info_idx,
        "warning should be rendered before info"
    );
}

#[test]
fn run_doctor_groups_same_severity_findings_in_alphabetical_order() {
    let root = temp_workspace("doctor-same-severity-order");
    let broken = create_workspace_dir(&root, "broken");

    write_root_manifest(
        &root,
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n",
    );
    fs::write(broken.join("effigy.toml"), "[tasks\nbad = true\n").expect("write bad manifest");

    let err = run_doctor_err_from_cwd(&root, false);

    let rendered = doctor_nonzero_rendered(err);

    let health_error_idx = rendered
        .find("health.task.execute")
        .expect("expected health execute error finding");
    let parse_error_idx = rendered
        .find("manifest.parse")
        .expect("expected manifest parse error finding");

    assert!(
        health_error_idx < parse_error_idx,
        "same-severity error groups should be ordered alphabetically by check_id"
    );
}
