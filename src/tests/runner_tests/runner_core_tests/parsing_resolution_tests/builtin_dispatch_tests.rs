use super::prelude::*;

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
    let farmyard = root.join("farmyard");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    write_manifest(
        &farmyard.join("effigy.toml"),
        r#"[catalog]
alias = "farmyard"
"#,
    );

    let out = run_builtin_ok(root, "farmyard/help", &[]);
    assert_output_contains_all(&out, &["Commands", "effigy help"]);
}
