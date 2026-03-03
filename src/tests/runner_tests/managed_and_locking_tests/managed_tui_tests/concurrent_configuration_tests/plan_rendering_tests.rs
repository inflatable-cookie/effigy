use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_supports_concurrent_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-concurrent-entries");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
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

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "Managed Task Plan",
            "profile: default",
            "tab-order: front, process-2, api",
            "printf api",
            "printf background",
            "printf front",
            "250",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_supports_single_definition_ordered_profile_entries() {
    let _guard = lock_test();
    let root = temp_workspace("managed-single-definition-ordered-profile");
    let _env = managed_tui_env();
    write_ranked_task_ref_manifest(&root, Some(1200));
    write_ranked_catalog_tasks(&root);

    assert_run_dev_with_repo_contains(
        &root,
        &[],
        &[
            "tab-order: dairy/dev, cream/dev, farmyard/api, farmyard/jobs",
            "start-after-ms",
            "1200",
        ],
    );
}
