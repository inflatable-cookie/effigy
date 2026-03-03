use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedPlanCase {
            workspace: "managed-tab-order",
        },
        ManagedPlanCase {
            workspace: "managed-tab-order-ranked",
        },
    ];

    for case in cases {
        let root = temp_workspace(case.workspace);
        write_ranked_name_manifest(&root);

        let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
        assert_contains_all(&out, &["tab-order: dairy, cream, api, jobs"]);
    }
}

#[test]
fn run_manifest_task_managed_tui_supports_ranked_tab_order_map_with_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-tab-order-ranked-refs");
    let _env = managed_tui_env();
    write_ranked_task_ref_manifest(&root, None);
    write_ranked_catalog_tasks(&root);

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &["tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs"],
    );
}
