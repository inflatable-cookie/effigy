use crate::runner::tests::prelude::execution::run_manifest_task_with_cwd;
use effigy_cli::TaskInvocation;

#[test]
fn run_array_retired_policies_fail_before_any_step() {
    for (key, value) in [
        ("depends_on", "[]"),
        ("retry", "0"),
        ("retry_delay_ms", "0"),
        ("fail_fast", "true"),
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("effigy.toml"),
            format!(
            "[tasks.validate]\nrun = ['touch should-not-exist', {{run='true', {key}={value}}}]\n"),
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
        let message = error.to_string();
        assert!(
            message.contains(key)
                && message.contains("removed in v0.15")
                && message.contains("Delete"),
            "{message}"
        );
        assert!(!root.path().join("should-not-exist").exists());
    }
}

#[test]
fn run_array_duplicate_ids_fail_before_any_step() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("effigy.toml"), "[tasks.validate]\nrun = [{id='same',run='touch should-not-exist'}, {id='same',run='true'}]\n").unwrap();
    let error = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "validate".into(),
            args: vec![],
        },
        root.path().to_path_buf(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("duplicate step id"));
    assert!(!root.path().join("should-not-exist").exists());
}
