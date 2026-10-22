use crate::runner::tests::prelude::{
    assert_managed_output_case_table, lock_test, managed_tui_env, write_root_manifest,
    ManagedInvocation, ManagedOutputCase, Path,
};

fn setup_shell_enabled(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 2 },
  { task = "shell", start = 2, tab = 1 }
]
"#,
    );
}

#[test]
fn run_manifest_task_managed_tui_shell_behavior_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [ManagedOutputCase {
        workspace: "managed-shell-enabled",
        invocation: ManagedInvocation::Dev,
        args: &[],
        expected: &[
            "shell",
            "exec ${SHELL:-/bin/zsh} -i",
            "tab-order: shell, api",
        ],
        expected_absent: &[],
        setup: setup_shell_enabled,
    }];

    assert_managed_output_case_table(&cases);
}
