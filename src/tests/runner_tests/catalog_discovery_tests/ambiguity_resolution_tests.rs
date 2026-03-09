use super::prelude::{
    assert_task_ambiguous_reset_db, create_workspace_dir, run_builtin_err, temp_workspace,
    write_catalog_tasks,
};

#[test]
fn run_manifest_task_unprefixed_reports_ambiguity_on_equal_shallow_depth() {
    let root = temp_workspace("ambiguous");
    let catalog_a = create_workspace_dir(&root, "catalog_a");
    let catalog_b = create_workspace_dir(&root, "catalog_b");

    write_catalog_tasks(
        &catalog_a,
        Some("catalog_a"),
        &[("reset-db", "printf catalog_a")],
    );
    write_catalog_tasks(&catalog_b, Some("catalog_b"), &[("reset-db", "printf catalog_b")]);

    let err = run_builtin_err(root, "reset-db", &[]);

    assert_task_ambiguous_reset_db(err);
}
