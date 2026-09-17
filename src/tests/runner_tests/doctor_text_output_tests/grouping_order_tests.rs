use crate::runner::tests::prelude::{
    assert_output_contains_all, doctor_nonzero_rendered, run_builtin_err, temp_workspace,
    write_root_manifest,
};

#[test]
fn run_doctor_verbose_text_output_includes_per_finding_entries() {
    let root = temp_workspace("doctor-verbose-entries");
    write_root_manifest(
        &root,
        r#"[tasks.alpha]
run = { task = "missing/task" }

[tasks.beta]
run = { task = "missing/task" }
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
    write_root_manifest(&root, "[tasks.alpha]\nrun = { task = \"missing/task\" }\n");

    let err = run_builtin_err(root, "doctor", &["--deep"]);

    let rendered = doctor_nonzero_rendered(err);

    let error_idx = rendered
        .find("tasks.references.resolve")
        .expect("expected error finding");
    let warning_idx = rendered
        .find("health.task.discovery")
        .expect("expected warning finding");

    assert!(
        error_idx < warning_idx,
        "error should be rendered before warning"
    );
    assert!(
        !rendered.contains("workspace.root-resolution"),
        "info-only sections should not be rendered in normal doctor text output"
    );
}

#[test]
fn run_doctor_groups_same_severity_findings_in_alphabetical_order() {
    let root = temp_workspace("doctor-same-severity-order");

    write_root_manifest(
        &root,
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n\n[tasks.alpha]\nrun = { task = \"missing/task\" }\n",
    );

    let err = run_builtin_err(root, "doctor", &["--deep"]);

    let rendered = doctor_nonzero_rendered(err);

    let health_error_idx = rendered
        .find("health.task.execute")
        .expect("expected health execute error finding");
    let parse_error_idx = rendered
        .find("tasks.references.resolve")
        .expect("expected task reference error finding");

    assert!(
        health_error_idx < parse_error_idx,
        "same-severity error groups should be ordered alphabetically by check_id"
    );
}
