use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_supports_relative_task_refs() {
    let _guard = lock_test();
    let root = temp_workspace("managed-relative-task-ref");
    let dairy = create_workspace_dir(&root, "dairy");
    let froyo = root.join("froyo");
    let _env = managed_tui_env();

    write_manifest(
        &dairy.join("effigy.toml"),
        r#"[catalog]
alias = "dairy"
[tasks.dev]
mode = "tui"
concurrent = [{ name = "validate-stack", task = "../froyo/validate" }]
"#,
    );
    write_froyo_validate_catalog(&root);

    let out = run_task_with_repo(&root, "dairy/dev", &[]).expect("managed plan should render");
    assert_contains_all(
        &out,
        &[
            "validate-stack",
            "froyo-validate",
            &froyo.display().to_string(),
        ],
    );
}
