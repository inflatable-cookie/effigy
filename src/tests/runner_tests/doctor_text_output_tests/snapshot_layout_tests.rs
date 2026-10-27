use crate::runner::tests::prelude::{
    assert_output_contains_all, assert_output_excludes_all, create_workspace_dir,
    doctor_nonzero_rendered, run_builtin_ok, run_doctor_err_from_cwd,
    setup_doctor_explain_catalog_workspace, temp_workspace, write_manifest, write_root_manifest,
};

#[test]
fn run_doctor_text_output_has_blank_line_between_sections() {
    let root = temp_workspace("doctor-section-spacing");
    write_root_manifest(&root, "[tasks.health]\nrun = \"printf ok\"\n");

    let out = run_builtin_ok(root, "doctor", &[]);

    assert!(out.starts_with("Doctor's Report\n"));
    assert_output_contains_all(&out, &["No findings.", "\nsummary  ok:"]);
    assert_output_excludes_all(&out, &["workspace.root-resolution"]);
    assert_output_excludes_all(&out, &["\n\nRoot Resolution\n"]);
}

#[test]
fn run_doctor_text_output_snapshot_mixed_findings_and_fix_actions() {
    let root = temp_workspace("doctor-text-snapshot-mixed");
    let catalog_a = create_workspace_dir(&root, "catalog_a");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"

[tasks.build]
run = { task = "missing/task" }
"#,
    );
    write_root_manifest(&root, "[catalog.members]\ncatalog_a = \"catalog_a\"\n");

    let err = run_doctor_err_from_cwd(&root, true);

    let rendered = doctor_nonzero_rendered(err);

    assert!(rendered.starts_with("Doctor's Report\n"));
    assert_output_contains_all(
        &rendered,
        &[
            "\n\nFix Actions\n",
            "status",
            "manifest.health_task_scaffold",
            "applied",
            "\n\nsummary  ok:",
        ],
    );

    assert!(rendered.contains("tasks.references.resolve"));
    assert!(!rendered.contains("workspace.root-resolution"));
    assert!(!rendered.contains("health.task.discovery"));
}
