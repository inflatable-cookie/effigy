use crate::runner::tests::prelude::{
    assert_managed_output_case_table, lock_test, managed_tui_env, write_ranked_catalog_tasks,
    write_ranked_name_manifest, write_ranked_task_ref_manifest, ManagedInvocation,
    ManagedOutputCase, Path,
};

fn setup_ranked_name_manifest(root: &Path) {
    write_ranked_name_manifest(root);
}

fn setup_ranked_task_refs(root: &Path) {
    write_ranked_task_ref_manifest(root, None);
    write_ranked_catalog_tasks(root);
}

#[test]
fn run_manifest_task_managed_tui_tab_order_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-tab-order",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["tab-order: catalog_b, catalog_c, api, jobs"],
            expected_absent: &[],
            setup: setup_ranked_name_manifest,
        },
        ManagedOutputCase {
            workspace: "managed-tab-order-ranked",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["tab-order: catalog_b, catalog_c, api, jobs"],
            expected_absent: &[],
            setup: setup_ranked_name_manifest,
        },
        ManagedOutputCase {
            workspace: "managed-tab-order-ranked-refs",
            invocation: ManagedInvocation::DevWithRepo,
            args: &[],
            expected: &["tab-order: catalog_b/dev, catalog_c/dev, catalog_a/api, catalog_a/jobs"],
            expected_absent: &[],
            setup: setup_ranked_task_refs,
        },
    ];

    assert_managed_output_case_table(&cases);
}
