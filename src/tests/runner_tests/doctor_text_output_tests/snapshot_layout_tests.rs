use super::prelude::*;

#[test]
fn run_doctor_text_output_has_blank_line_between_sections() {
    let root = temp_workspace("doctor-section-spacing");
    write_root_manifest(&root, "[tasks.health]\nrun = \"printf ok\"\n");

    let out = run_builtin_ok(root, "doctor", &[]);

    assert!(out.starts_with("Doctor's Report\n"));
    assert_output_contains_all(&out, &["workspace.root-resolution", "\n\nsummary  ok:"]);
    assert_output_excludes_all(&out, &["\n\nRoot Resolution\n"]);
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
