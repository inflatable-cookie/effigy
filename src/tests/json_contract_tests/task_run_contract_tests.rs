use super::prelude::{execution::*, harness::*, json::*, runtime::*};

#[test]
fn task_run_json_contract_reclaims_stale_lock_and_remains_valid_payload() {
    let root = temp_workspace("task-run-json-stale-lock-reclaim");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(
        root.join(".effigy/locks/workspace.lock"),
        r#"{"scope":"workspace","pid":999999,"started_at_epoch_ms":0}"#,
    )
    .expect("write stale lock");

    let parsed = run_invocation_json(root, "build", &["--json"]);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["exit_code"], 0);
}

#[test]
fn catalog_task_run_json_contract_success_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-success");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf build-ok\"\n",
    );

    let parsed = run_invocation_json(root, "build", &["--json"]);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "build");
    assert_eq!(parsed["exit_code"], 0);
    assert_eq!(parsed["stdout"], "build-ok");
}

#[test]
fn catalog_task_run_json_contract_failure_has_versioned_shape() {
    let root = temp_workspace("task-run-json-contract-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.fail]\nrun = \"sh -lc 'printf fail-out; printf fail-err >&2; exit 9'\"\n",
    );

    let err = run_manifest_task_with_cwd(
        &TaskInvocation {
            name: "fail".to_owned(),
            args: vec!["--json".to_owned()],
        },
        root,
    )
    .expect_err("expected non-zero task failure");

    let rendered = match err {
        RunnerError::CommandJsonFailure { rendered } => rendered,
        other => panic!("unexpected error: {other}"),
    };
    let parsed = parse_json(&rendered);
    assert_schema_v1(&parsed, "effigy.task.run.v1");
    assert_eq!(parsed["ok"], false);
    assert_eq!(parsed["task"], "fail");
    assert_eq!(parsed["exit_code"], 9);
    assert_eq!(parsed["stdout"], "fail-out");
    assert_eq!(parsed["stderr"], "fail-err");
}
