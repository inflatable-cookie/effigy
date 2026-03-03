use super::super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_appends_shell_process_when_enabled() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-enabled");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/zsh} -i"]);
}

#[test]
fn run_manifest_task_managed_tui_uses_global_shell_run_override() {
    let _guard = lock_test();
    let root = temp_workspace("managed-shell-global-override");
    let _env = managed_tui_env();
    write_root_manifest(
        &root,
        r#"[shell]
run = "exec ${SHELL:-/bin/bash} -i"

[tasks.dev]
mode = "tui"
shell = true
concurrent = [{ name = "api", run = "printf api" }]
"#,
    );

    let out = run_dev(&root, &[]).expect("managed plan should include configured shell process");
    assert_contains_all(&out, &["shell", "exec ${SHELL:-/bin/bash} -i"]);
}
