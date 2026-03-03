use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_processes_can_reference_other_tasks() {
    let _guard = lock_test();
    let root = temp_workspace("managed-task-refs");
    let _env = managed_tui_env();
    let farmyard = root.join("farmyard");
    let cream = root.join("cream");

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", task = "farmyard/api" },
  { name = "front", task = "cream/dev" }
]
"#,
    );
    write_farmyard_and_cream_dev_catalogs(&root);

    let out = run_dev(&root, &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "farmyard-api",
            "cream-dev",
            &farmyard.display().to_string(),
            &cream.display().to_string(),
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_supports_compact_profile_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-compact-profile-refs");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ task = "farmyard/api" }, { task = "cream/dev" }]

[tasks.dev.profiles.admin]
concurrent = [{ task = "farmyard/api" }]
"#,
    );
    write_farmyard_and_cream_dev_catalogs(&root);

    let out = run_dev_with_repo(&root, &[]).expect("managed compact plan should render");
    assert_contains_all(
        &out,
        &[
            "profile: default",
            "farmyard-api",
            "cream-dev",
            "farmyard/api",
            "cream/dev",
        ],
    );
}

#[test]
fn run_manifest_task_managed_tui_process_run_array_supports_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-process-run-array");
    let farmyard = create_workspace_dir(&root, "farmyard");
    let _env = managed_tui_env();

    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "combo", task = "combo" }]

[tasks.combo]
run = ["printf start", { task = "farmyard/api" }, "printf done"]
"#,
    );
    write_catalog_tasks(
        &farmyard,
        Some("farmyard"),
        &[("api", "printf farmyard-api")],
    );

    let out = run_dev_with_repo(&root, &[]).expect("managed plan should render");
    assert_contains_all(&out, &["printf start", "farmyard-api", "printf done", "cd"]);
}
