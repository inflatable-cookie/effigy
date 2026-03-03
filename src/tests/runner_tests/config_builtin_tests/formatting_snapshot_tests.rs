use super::prelude::*;

#[test]
fn run_manifest_task_builtin_config_has_blank_line_between_sections() {
    let root = workspace_with_empty_manifest("builtin-config-section-spacing");

    let out = run_config_ok(root, &[]);
    assert_contains_all(
        &out,
        &["\n\nGlobal\n", "\n\nBuilt-in Test\n", "\n\nTasks\n"],
    );
}
