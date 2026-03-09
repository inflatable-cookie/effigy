use super::prelude::{
    assert_output_contains_all, assert_task_invocation_error_contains, run_builtin_err,
    run_builtin_ok, setup_doctor_explain_catalog_workspace, temp_workspace, write_root_manifest,
};

#[test]
fn run_doctor_explain_text_reports_resolution_selection() {
    let root = setup_doctor_explain_catalog_workspace("doctor-explain-selection");

    let out = run_builtin_ok(root, "doctor", &["catalog_a/build"]);

    assert_output_contains_all(
        &out,
        &[
            "Doctor Explain",
            "selection-status: ok",
            "selected-catalog: catalog_a",
            "selected-mode: explicit_prefix",
            "selection-reasoning:",
            "candidate-catalogs",
            "selection-evidence",
        ],
    );
}

#[test]
fn run_doctor_explain_text_reports_deferral_reasoning_on_missing_task() {
    let root = temp_workspace("doctor-explain-deferral");
    write_root_manifest(&root, "[defer]\nrun = \"printf deferred\"\n");

    let out = run_builtin_ok(root, "doctor", &["missing-task"]);

    assert_output_contains_all(
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
