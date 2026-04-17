use crate::runner::tests::prelude::{
    builtin_test_max_parallel, discover_catalogs, parse_task_runtime_args, temp_workspace,
    write_root_manifest, PathBuf, RunnerError, TaskRuntimeArgs,
};

#[test]
fn parse_task_runtime_args_extracts_repo_verbose_and_passthrough() {
    let args = vec![
        "--repo".to_owned(),
        "/tmp/x".to_owned(),
        "--env-schema".to_owned(),
        "config/test.env.schema".to_owned(),
        "--verbose-root".to_owned(),
        "--flag".to_owned(),
        "abc".to_owned(),
    ];
    let parsed = parse_task_runtime_args(&args).expect("parse");
    assert_eq!(
        parsed,
        TaskRuntimeArgs {
            repo_override: Some(PathBuf::from("/tmp/x")),
            verbose_root: true,
            env_schema_override: Some(PathBuf::from("config/test.env.schema")),
            passthrough: vec!["--flag".to_owned(), "abc".to_owned()],
        }
    );
}

#[test]
fn parse_task_runtime_args_requires_env_schema_value() {
    let err = parse_task_runtime_args(&["--env-schema".to_owned()]).expect_err("parse should fail");
    match err {
        RunnerError::TaskInvocation(message) => {
            assert!(message.contains("task argument --env-schema requires a value"));
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn builtin_test_max_parallel_reads_root_manifest_config() {
    let root = temp_workspace("builtin-test-max-parallel-config");
    write_root_manifest(
        &root,
        r#"[test]
max_parallel = 1
"#,
    );
    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert_eq!(builtin_test_max_parallel(&catalogs, &root), 1);
}

#[test]
fn builtin_test_max_parallel_falls_back_when_invalid_or_missing() {
    let root = temp_workspace("builtin-test-max-parallel-default");
    write_root_manifest(
        &root,
        r#"[test]
max_parallel = 0
"#,
    );
    let catalogs = discover_catalogs(&root).expect("discover catalogs");
    assert_eq!(builtin_test_max_parallel(&catalogs, &root), 3);
}
