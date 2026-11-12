use crate::runner::tests::prelude::{
    install_fake_container_runtime, lock_test, managed_plan_process_row, managed_tui_env, run_dev,
    temp_workspace, write_root_manifest, Path,
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
    let root = temp_workspace("managed-shell-enabled");
    setup_shell_enabled(&root);
    let _runtime = install_fake_container_runtime(&root);
    let output = run_dev(&root, &[]).expect("managed plan should render");

    assert!(output.lines().any(|line| line == "tab-order: shell, api"));
    assert_eq!(
        managed_plan_process_row(&output, "shell"),
        vec![
            "shell".to_owned(),
            "standard".to_owned(),
            root.display().to_string(),
            "disabled".to_owned(),
            "exec ${SHELL:-/bin/zsh} -i".to_owned(),
            "0".to_owned(),
            "disabled".to_owned(),
        ]
    );
}
