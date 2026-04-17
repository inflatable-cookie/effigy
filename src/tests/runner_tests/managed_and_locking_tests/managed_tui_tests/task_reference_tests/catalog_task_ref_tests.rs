use crate::runner::tests::prelude::{
    assert_managed_output_case_table, assert_managed_output_derived_case_table,
    create_workspace_dir, lock_test, managed_tui_env, write_catalog_a_and_catalog_c_dev_catalogs,
    write_catalog_tasks, write_managed_tui_dev_manifest, write_managed_tui_dev_manifest_with_extra,
    ManagedInvocation, ManagedOutputCase, ManagedOutputDerivedCase, Path,
};

fn setup_task_refs_and_catalogs(root: &Path) {
    write_managed_tui_dev_manifest(
        root,
        r#"[
  { name = "api", task = "catalog_a/api" },
  { name = "front", task = "catalog_c/dev" }
]
"#,
    );
    write_catalog_a_and_catalog_c_dev_catalogs(root);
}

fn setup_compact_profile_task_refs(root: &Path) {
    write_managed_tui_dev_manifest_with_extra(
        root,
        r#"[{ task = "catalog_a/api" }, { task = "catalog_c/dev" }]"#,
        r#"[tasks.dev.profiles.admin]
concurrent = [{ task = "catalog_a/api" }]"#,
    );
    write_catalog_a_and_catalog_c_dev_catalogs(root);
}

fn setup_process_run_array_task_refs(root: &Path) {
    let catalog_a = create_workspace_dir(root, "catalog_a");
    write_managed_tui_dev_manifest_with_extra(
        root,
        r#"[{ name = "combo", task = "combo" }]"#,
        r#"[tasks.combo]
run = ["printf start", { task = "catalog_a/api" }, "printf done"]"#,
    );
    write_catalog_tasks(
        &catalog_a,
        Some("catalog_a"),
        &[("api", "printf catalog_a-api")],
    );
}

fn expected_catalog_paths(root: &Path) -> Vec<String> {
    vec![
        root.join("catalog_a").display().to_string(),
        root.join("catalog_c").display().to_string(),
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
        expected: &["catalog_a-api", "catalog_c-dev"],
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
                "catalog_a-api",
                "catalog_c-dev",
                "catalog_a/api",
                "catalog_c/dev",
            ],
            expected_absent: &[],
            setup: setup_compact_profile_task_refs,
        },
        ManagedOutputCase {
            workspace: "managed-process-run-array",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["printf start", "catalog_a-api", "printf done", "cd"],
            expected_absent: &[],
            setup: setup_process_run_array_task_refs,
        },
    ];

    assert_managed_output_case_table(&cases);
}
