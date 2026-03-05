use super::super::prelude::*;

fn setup_shell_enabled(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );
}

fn setup_shell_global_override(root: &Path) {
    write_root_manifest(
        root,
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );
}

#[test]
fn run_manifest_task_managed_tui_shell_behavior_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [
        ManagedOutputCase {
            workspace: "managed-shell-enabled",
            invocation: ManagedInvocation::Dev,
            args: &[],
            expected: &["shell", "exec ${SHELL:-/bin/zsh} -i"],
            expected_absent: &[],
            setup: setup_shell_enabled,
        },
        ManagedOutputCase {
            workspace: "managed-shell-global-override",
            invocation: ManagedInvocation::Dev,
            args: &[],
            expected: &["shell", "exec ${SHELL:-/bin/bash} -i"],
            expected_absent: &[],
            setup: setup_shell_global_override,
        },
    ];

    assert_managed_output_case_table(&cases);
}
