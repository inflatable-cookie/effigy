use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use effigy_cli::TaskInvocation;

#[test]
fn run_array_order_and_timeout_use_child_runs() {
    let _guard = crate::contract_test_support::lock_test();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("effigy.toml"),
        r#"[tasks.validate]
run = ["printf a >> effects", {id="slow",run="sleep 5", timeout_ms=100}, "printf b >> effects"]
"#,
    )
    .unwrap();
    let error = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".into(),
            args: vec![],
        },
        root.path().to_path_buf(),
    )
    .unwrap_err();
    assert_eq!(error.task_exit_status(), Some(124));
    assert_eq!(
        std::fs::read_to_string(root.path().join("effects")).unwrap(),
        "a"
    );
}
