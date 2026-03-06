use super::prelude::{
    assert_task_ambiguous_reset_db, create_workspace_dir, run_builtin_err, temp_workspace,
    write_catalog_tasks,
};

#[test]
fn run_manifest_task_unprefixed_reports_ambiguity_on_equal_shallow_depth() {
    let root = temp_workspace("ambiguous");
    let farmyard = create_workspace_dir(&root, "farmyard");
    let dairy = create_workspace_dir(&root, "dairy");

    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("reset-db", "printf farmyard")],
    );
    write_catalog_tasks(&dairy, Some("dairy"), &[("reset-db", "printf dairy")]);

    let err = run_builtin_err(root, "reset-db", &[]);

    assert_task_ambiguous_reset_db(err);
}
