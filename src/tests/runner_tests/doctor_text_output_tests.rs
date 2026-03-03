use super::*;

fn write_root_manifest(root: &PathBuf, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn doctor_nonzero_rendered(err: RunnerError) -> String {
    match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    }
}

fn create_workspace_dir(root: &PathBuf, name: &str) -> PathBuf {
    let dir = root.join(name);
    fs::create_dir_all(&dir).expect("mkdir workspace dir");
    dir
}

fn run_doctor_err_from_cwd(root: &PathBuf, fix: bool) -> RunnerError {
    with_cwd(root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should fail")
}

fn setup_doctor_explain_catalog_workspace(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    let farmyard = create_workspace_dir(&root, "farmyard");
    write_root_manifest(&root, "[tasks.root]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.build]\nrun = \"printf farmyard\"\n",
    );
    root
}

#[test]
fn run_doctor_text_output_has_blank_line_between_sections() {
    let root = temp_workspace("doctor-section-spacing");
    write_root_manifest(&root, "[tasks.health]\nrun = \"printf ok\"\n");

    let out = run_builtin_ok(root, "doctor", &[]);

    assert!(out.starts_with("Doctor's Report\n"));
    assert_contains_all(&out, &["workspace.root-resolution", "\n\nsummary  ok:"]);
    assert!(!out.contains("\n\nRoot Resolution\n"));
}

#[test]
fn run_doctor_explain_text_reports_resolution_selection() {
    let root = setup_doctor_explain_catalog_workspace("doctor-explain-selection");

    let out = run_builtin_ok(root, "doctor", &["farmyard/build"]);

    assert_contains_all(
        &out,
        &[
            "Doctor Explain",
            "selection-status: ok",
            "selected-catalog: farmyard",
            "selected-mode: explicit_prefix",
            "selection-reasoning:",
            "candidate-catalogs",
            "selection-evidence",
        ],
    );
}

#[test]
fn run_doctor_explain_text_snapshot_prefix_block_is_stable() {
    let root = setup_doctor_explain_catalog_workspace("doctor-explain-snapshot-prefix");

    let out = run_builtin_ok(root, "doctor", &["farmyard/build", "--", "--watch"]);

    let (prefix_block, _) = out
        .split_once("\ncandidate-catalogs:\n")
        .expect("expected candidate-catalogs section");
    let lines = prefix_block.lines().collect::<Vec<&str>>();
    assert_eq!(lines.len(), 12);
    assert_eq!(lines[0], "Doctor Explain");
    assert_eq!(lines[1], "──────────────");
    assert_eq!(lines[2], "request: farmyard/build");
    assert_eq!(lines[3], "args: -- --watch");
    assert!(
        lines[4].starts_with("resolved-root: "),
        "resolved-root line changed: {}",
        lines[4]
    );
    assert!(
        lines[4].contains("doctor-explain-snapshot-prefix"),
        "resolved-root should include workspace marker: {}",
        lines[4]
    );
    assert_eq!(lines[5], "selection-status: ok");
    assert_eq!(lines[6], "selected-catalog: farmyard");
    assert_eq!(lines[7], "selected-mode: explicit_prefix");
    assert_eq!(
        lines[8],
        "selection-reasoning: selected catalog by explicit task prefix"
    );
    assert_eq!(lines[9], "deferral-considered: false");
    assert_eq!(lines[10], "deferral-selected: false");
    assert_eq!(
        lines[11],
        "deferral-reasoning: deferral was not considered because the selection outcome does not trigger deferral"
    );
}

#[test]
fn run_doctor_explain_text_reports_deferral_reasoning_on_missing_task() {
    let root = temp_workspace("doctor-explain-deferral");
    write_root_manifest(&root, "[defer]\nrun = \"printf deferred\"\n");

    let out = run_builtin_ok(root, "doctor", &["missing-task"]);

    assert_contains_all(
        &out,
        &[
            "Doctor Explain",
            "selection-status: error",
            "deferral-considered: true",
            "deferral-selected: true",
            "deferral-reasoning:",
            "deferral-source",
        ],
    );
}

#[test]
fn run_doctor_explain_rejects_fix_mode() {
    let root = temp_workspace("doctor-explain-fix-invalid");
    write_root_manifest(&root, "");

    let err = run_builtin_err(root, "doctor", &["--fix", "build"]);
    assert_task_invocation_error_contains(err, &["`--fix` is not supported with explain mode"]);
}

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

    assert_contains_all(
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

#[test]
fn run_doctor_text_output_snapshot_mixed_findings_and_fix_actions() {
    let root = temp_workspace("doctor-text-snapshot-mixed");
    let farmyard = create_workspace_dir(&root, "farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"

[tasks.build]
run = [{ task = "missing/task" }]
"#,
    );

    let err = run_doctor_err_from_cwd(&root, true);

    let rendered = doctor_nonzero_rendered(err);

    assert!(rendered.starts_with("Doctor's Report\n"));
    assert_contains_all(
        &rendered,
        &[
            "\n\nFix Actions\n",
            "status",
            "manifest.health_task_scaffold",
            "applied",
            "\n\nsummary  ok:",
        ],
    );

    let error_idx = rendered
        .find("tasks.references.resolve")
        .expect("expected error check");
    let discovery_idx = rendered
        .find("health.task.discovery")
        .expect("expected health discovery check");
    let info_idx = rendered
        .find("workspace.root-resolution")
        .expect("expected info check");
    assert!(error_idx < discovery_idx);
    assert!(discovery_idx < info_idx);
}
