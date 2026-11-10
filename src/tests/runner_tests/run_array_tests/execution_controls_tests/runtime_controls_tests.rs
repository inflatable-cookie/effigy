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

#[test]
fn nested_host_deadlines_take_the_minimum_and_skip_later_siblings() {
    let _guard = crate::contract_test_support::lock_test();
    for (parent, child) in [(1000, 5000), (5000, 1000)] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("effigy.toml"),
            format!(
                r#"
[tasks.outer]
scratch = false
env = {{ SCOPED = "inherited" }}
run = [{{ task = "middle", timeout_ms = {parent} }}, "printf outer-late >> effects"]
[tasks.middle]
scratch = false
run = [{{ task = "leaf", timeout_ms = {child} }}, "printf middle-late >> effects"]
[tasks.leaf]
scratch = false
run = 'printf "$SCOPED" >> effects; sleep 20; printf leaf-late >> effects'
"#
            ),
        )
        .unwrap();
        let started = std::time::Instant::now();
        let error = run_manifest_task_with_cwd(
            &TaskInvocation {
                name: "outer".into(),
                args: vec![],
            },
            root.path().into(),
        )
        .unwrap_err();
        assert_eq!(
            error.task_exit_status(),
            Some(124),
            "parent={parent}, child={child}: {error}"
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert_eq!(
            std::fs::read_to_string(root.path().join("effects")).unwrap(),
            "inherited"
        );
    }
}
