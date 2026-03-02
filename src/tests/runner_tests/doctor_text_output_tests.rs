use super::*;

#[test]
fn run_doctor_text_output_has_blank_line_between_sections() {
    let root = temp_workspace("doctor-section-spacing");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf ok\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: Vec::new(),
        },
        root,
    )
    .expect("run doctor");

    assert!(out.starts_with("Doctor's Report\n"));
    assert!(out.contains("workspace.root-resolution"));
    assert!(out.contains("\n\nsummary  ok:"));
    assert!(!out.contains("\n\nRoot Resolution\n"));
}

#[test]
fn run_doctor_explain_text_reports_resolution_selection() {
    let root = temp_workspace("doctor-explain-selection");
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

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec!["farmyard/build".to_owned()],
        },
        root,
    )
    .expect("doctor explain run");

    assert!(out.contains("Doctor Explain"));
    assert!(out.contains("selection-status: ok"));
    assert!(out.contains("selected-catalog: farmyard"));
    assert!(out.contains("selected-mode: explicit_prefix"));
    assert!(out.contains("selection-reasoning:"));
    assert!(out.contains("candidate-catalogs"));
    assert!(out.contains("selection-evidence"));
}

#[test]
fn run_doctor_explain_text_snapshot_prefix_block_is_stable() {
    let root = temp_workspace("doctor-explain-snapshot-prefix");
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

    let out = run_manifest_task_with_cwd(
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
    .expect("doctor explain run");

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
    write_manifest(
        &root.join("effigy.toml"),
        "[defer]\nrun = \"printf deferred\"\n",
    );

    let out = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec!["missing-task".to_owned()],
        },
        root,
    )
    .expect("doctor explain missing");

    assert!(out.contains("Doctor Explain"));
    assert!(out.contains("selection-status: error"));
    assert!(out.contains("deferral-considered: true"));
    assert!(out.contains("deferral-selected: true"));
    assert!(out.contains("deferral-reasoning:"));
    assert!(out.contains("deferral-source"));
}

#[test]
fn run_doctor_explain_rejects_fix_mode() {
    let root = temp_workspace("doctor-explain-fix-invalid");
    write_manifest(&root.join("effigy.toml"), "");

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec!["--fix".to_owned(), "build".to_owned()],
        },
        root,
    )
    .expect_err("doctor explain --fix should fail");

    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("`--fix` is not supported with explain mode"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn run_doctor_verbose_text_output_includes_per_finding_entries() {
    let root = temp_workspace("doctor-verbose-entries");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[tasks.alpha]
run = [{ task = "missing/task" }]

[tasks.beta]
run = [{ task = "missing/task" }]
"#,
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "doctor".to_owned(),
            args: vec!["--verbose".to_owned()],
        },
        root,
    )
    .expect_err("doctor should fail for unresolved task references");

    let rendered = match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };

    assert!(rendered.contains("tasks.references.resolve"));
    assert!(rendered.contains("findings: 2"));
    assert!(rendered.contains("entry: 1"));
    assert!(rendered.contains("entry: 2"));
    assert!(rendered.contains("entry-evidence"));
    assert!(rendered.contains("entry-remediation"));
}

#[test]
fn run_doctor_groups_findings_in_severity_first_order() {
    let root = temp_workspace("doctor-severity-order");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\nunknown_key = true\n",
    );

    let err = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: false,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should fail for unsupported manifest key");

    let rendered = match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };

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
    let broken = root.join("broken");
    fs::create_dir_all(&broken).expect("mkdir broken");

    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n",
    );
    fs::write(broken.join("effigy.toml"), "[tasks\nbad = true\n").expect("write bad manifest");

    let err = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: false,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should fail for health execution and parse errors");

    let rendered = match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };

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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"

[tasks.build]
run = [{ task = "missing/task" }]
"#,
    );

    let err = with_cwd(&root, || {
        run_doctor(DoctorArgs {
            repo_override: None,
            output_json: false,
            fix: true,
            verbose: false,
            explain: None,
        })
    })
    .expect_err("doctor should fail with unresolved task reference");

    let rendered = match err {
        RunnerError::DoctorNonZero { rendered, .. } => rendered,
        other => panic!("unexpected error: {other}"),
    };

    assert!(rendered.starts_with("Doctor's Report\n"));
    assert!(rendered.contains("\n\nFix Actions\n"));
    assert!(rendered.contains("status"));
    assert!(rendered.contains("manifest.health_task_scaffold"));
    assert!(rendered.contains("applied"));
    assert!(rendered.contains("\n\nsummary  ok:"));

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
