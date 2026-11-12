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
    let catalog_rows = out
        .lines()
        .filter(|line| line.starts_with("-  :") || line.starts_with("- catalog_a :"))
        .collect::<Vec<_>>();
    assert_eq!(catalog_rows, ["- catalog_a : catalog_a/effigy.toml"]);
    assert_eq!(
        out.lines()
            .filter(|line| line.starts_with("- dev :") || line.starts_with("- catalog_a/reset-db :"))
            .collect::<Vec<_>>(),
        [
            "- dev : effigy.toml",
            "- catalog_a/reset-db : catalog_a/effigy.toml",
        ]
    );
    for signature in ["      printf root", "      printf catalog_a"] {
        assert!(out.lines().any(|line| line == signature), "{out}");
    }
}

#[test]
fn run_tasks_without_catalogs_still_lists_builtin_tasks() {
    let root = temp_workspace("builtins-only");

    let out = run_tasks_from_repo(&root, None, None, false);
    for line in [
        "count: 0",
        "• info: none",
        "Built-in Tasks",
        "- help : Show general help (same as --help)",
        "- doctor : Built-in remedial health checks for environment, manifests, and task references",
        "- test : Built-in test runner detection, supports <catalog>/test fallback, optional --plan",
        "- tasks : List effective catalogs and available tasks",
    ] {
        assert!(out.lines().any(|actual| actual == line), "missing {line:?}: {out}");
    }
    assert_eq!(
        out.lines().filter(|line| line.starts_with("-  :")).count(),
        0
    );
    let task_section = out
        .split_once("\nTasks\n")
        .expect("Tasks section")
        .1
        .split_once("\nBuilt-in Tasks\n")
        .expect("Built-in Tasks section")
        .0;
    assert!(task_section.lines().any(|line| line == "• info: none"));
    assert!(!task_section.lines().any(|line| line.starts_with("- ")));
}
