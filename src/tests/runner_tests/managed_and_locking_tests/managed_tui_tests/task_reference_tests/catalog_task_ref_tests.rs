use super::prelude::{
    assert_managed_output_case_table, assert_managed_output_derived_case_table,
    create_workspace_dir, lock_test, managed_tui_env, write_catalog_tasks,
    write_farmyard_and_cream_dev_catalogs, write_managed_tui_dev_manifest,
    write_managed_tui_dev_manifest_with_extra, ManagedInvocation, ManagedOutputCase,
    ManagedOutputDerivedCase, Path,
};

fn setup_task_refs_and_catalogs(root: &Path) {
    write_managed_tui_dev_manifest(
        root,
        r#"[
  { name = "api", task = "farmyard/api" },
  { name = "front", task = "cream/dev" }
]
"#,
    );
    write_farmyard_and_cream_dev_catalogs(root);
}

fn setup_compact_profile_task_refs(root: &Path) {
    write_managed_tui_dev_manifest_with_extra(
        root,
        r#"[{ task = "farmyard/api" }, { task = "cream/dev" }]"#,
        r#"[tasks.dev.profiles.admin]
concurrent = [{ task = "farmyard/api" }]"#,
    );
    write_farmyard_and_cream_dev_catalogs(root);
}

fn setup_process_run_array_task_refs(root: &Path) {
    let farmyard = create_workspace_dir(root, "farmyard");
    write_managed_tui_dev_manifest_with_extra(
        root,
        r#"[{ name = "combo", task = "combo" }]"#,
        r#"[tasks.combo]
run = ["printf start", { task = "farmyard/api" }, "printf done"]"#,
    );
    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("api", "printf farmyard-api")],
    );
}

fn expected_catalog_paths(root: &Path) -> Vec<String> {
    vec![
        root.join("farmyard").display().to_string(),
        root.join("cream").display().to_string(),
    ]
}

#[test]
fn run_manifest_task_managed_tui_processes_can_reference_other_tasks() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [ManagedOutputDerivedCase {
        workspace: "managed-task-refs",
        invocation: ManagedInvocation::Dev,
        args: &[],
        expected: &["farmyard-api", "cream-dev"],
        expected_absent: &[],
        expected_derived: expected_catalog_paths,
        setup: setup_task_refs_and_catalogs,
    }];

    assert_managed_output_derived_case_table(&cases);
}

#[test]
fn run_manifest_task_managed_tui_catalog_task_ref_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-compact-profile-refs",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "profile: default",
                "farmyard-api",
                "cream-dev",
                "farmyard/api",
                "cream/dev",
            ],
            expected_absent: &[],
            setup: setup_compact_profile_task_refs,
        },
        ManagedOutputCase {
            workspace: "managed-process-run-array",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["printf start", "farmyard-api", "printf done", "cd"],
            expected_absent: &[],
            setup: setup_process_run_array_task_refs,
        },
    ];

    assert_managed_output_case_table(&cases);
}
