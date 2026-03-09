use super::prelude::{
    assert_builtin_ok_empty, assert_output_contains_all, run_tasks_from_repo,
    setup_root_and_catalog_a_catalog, temp_workspace, write_root_manifest,
};

#[test]
fn run_tasks_lists_catalogs_and_tasks() {
    let root = setup_root_and_catalog_a_catalog("list-tasks");

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(&out, &["root", "catalog_a", "reset-db"]);
}

#[test]
fn run_tasks_supports_compact_task_definitions() {
    let root = temp_workspace("compact-tasks");
    write_root_manifest(
        &root,
        r#"[tasks]
api = "printf api"
jobs = "printf jobs"
"#,
    );

    let out = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(&out, &["api", "jobs", "printf api"]);
}

#[test]
fn run_tasks_supports_mixed_compact_and_table_task_definitions() {
    let root = temp_workspace("mixed-compact-and-table");
    write_root_manifest(
        &root,
        r#"[tasks]
api = "printf api"

[tasks.dev]
run = "printf dev"
"#,
    );

    assert_builtin_ok_empty(root.to_path_buf(), "api", &[]);

    let tasks = run_tasks_from_repo(&root, None, None, false);
    assert_output_contains_all(&tasks, &["api", "dev"]);
}

#[test]
fn run_tasks_supports_compact_sequence_task_definitions() {
    let root = temp_workspace("compact-sequence-tasks");
    write_root_manifest(
        &root,
        r#"[tasks]
drop-db = "printf drop-db"
migrate-db = "printf migrate-db"
reset-db = [{ task = "drop-db" }, { task = "migrate-db" }]
"#,
    );

    assert_builtin_ok_empty(root.to_path_buf(), "reset-db", &[]);

    let tasks = run_tasks_from_repo(&root, Some("reset-db"), None, false);
    assert_output_contains_all(&tasks, &["reset-db", "<sequence:2>"]);
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
