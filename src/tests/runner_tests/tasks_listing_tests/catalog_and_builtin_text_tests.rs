use crate::runner::tests::prelude::{
    assert_builtin_ok_empty, assert_output_contains_all, run_tasks_from_repo,
    setup_root_and_catalog_a_catalog, temp_workspace, write_root_manifest,
};

#[test]
fn run_tasks_supports_compact_mixed_and_sequence_definitions() {
    let root = temp_workspace("compact-mixed-sequence-tasks");
    write_root_manifest(
        &root,
        r#"[tasks]
api = "printf api"
jobs = "printf jobs"
drop-db = "printf drop-db"
migrate-db = "printf migrate-db"
reset-db = [{ task = "drop-db" }, { task = "migrate-db" }]

[tasks.dev]
run = "printf dev"
"#,
    );

    let tasks = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(
        &tasks,
        &[
            "api",
            "jobs",
            "printf api",
            "dev",
            "reset-db",
            "<sequence:2>",
        ],
    );

    assert_builtin_ok_empty(root.to_path_buf(), "api", &[]);
    assert_builtin_ok_empty(root.to_path_buf(), "reset-db", &[]);
}

#[test]
fn run_tasks_lists_catalogs_and_tasks() {
    let root = setup_root_and_catalog_a_catalog("list-tasks");

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(&out, &["root", "catalog_a", "reset-db"]);
}

#[test]
fn run_tasks_without_catalogs_still_lists_builtin_tasks() {
    let root = temp_workspace("builtins-only");

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(
        &out,
        &[
            "Tasks",
            "help",
            "doctor",
            "test",
            "<catalog>/test fallback",
            "tasks",
        ],
    );
}
