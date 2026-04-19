use crate::runner::tests::prelude::{
    assert_builtin_error_case_table, assert_output_contains_all, fs, run_builtin_ok,
    temp_workspace, write_manifest, BuiltinErrorCase,
};

#[test]
fn run_manifest_task_removed_builtins_show_migration_message() {
    let cases = [
        BuiltinErrorCase {
            workspace: "repo-pulse-migration-message",
            command: "repo-pulse",
            args: &[],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["no longer a built-in command", "effigy doctor"],
        },
        BuiltinErrorCase {
            workspace: "health-migration-message",
            command: "health",
            args: &[],
            manifest: "[tasks.build]\nrun = \"printf ok\"\n",
            expected: &["no longer a built-in command", "define `tasks.health`"],
        },
    ];

    assert_builtin_error_case_table(&cases);
}

#[test]
fn run_manifest_task_prefixed_builtin_help_is_supported() {
    let root = temp_workspace("builtin-help-prefixed-catalog");
    let catalog_a = root.join("catalog_a");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    write_manifest(&root.join("effigy.toml"), "");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        r#"[catalog]
alias = "catalog_a"
"#,
    );

    let out = run_builtin_ok(root, "catalog_a/help", &[]);
    assert_output_contains_all(&out, &["Commands", "effigy help"]);
}
