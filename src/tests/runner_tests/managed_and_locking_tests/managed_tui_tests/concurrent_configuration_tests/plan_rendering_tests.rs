use super::prelude::{
    assert_managed_output_case_table, lock_test, managed_tui_env, write_ranked_catalog_tasks,
    write_ranked_task_ref_manifest, write_root_manifest, ManagedInvocation, ManagedOutputCase,
    Path,
};

fn setup_concurrent_entries(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", start = 1, tab = 3 },
  { run = "printf background", start = 2, tab = 2, start_after_ms = 250 },
  { task = "front", start = 3, tab = 1 }
]

[tasks.api]
run = "printf api"

[tasks.front]
run = "printf front"
"#,
    );
}

fn setup_single_definition_ordered_profile_entries(root: &Path) {
    write_ranked_task_ref_manifest(root, Some(1200));
    write_ranked_catalog_tasks(root);
}

#[test]
fn run_manifest_task_managed_tui_concurrent_plan_rendering_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-concurrent-entries",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "Managed Task Plan",
                "profile: default",
                "tab-order: front, process-2, api",
                "printf api",
                "printf background",
                "printf front",
                "250",
            ],
            expected_absent: &[],
            setup: setup_concurrent_entries,
        },
        ManagedOutputCase {
            workspace: "managed-single-definition-ordered-profile",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &[
                "tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs",
                "start-after-ms",
                "1200",
            ],
            expected_absent: &[],
            setup: setup_single_definition_ordered_profile_entries,
        },
    ];

    assert_managed_output_case_table(&cases);
}
