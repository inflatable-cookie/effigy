use super::prelude::*;

#[test]
fn parse_task_runtime_args_extracts_repo_verbose_and_passthrough() {
    let args = vec![
        "--repo".to_owned(),
        "/tmp/x".to_owned(),
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
            passthrough: vec!["--flag".to_owned(), "abc".to_owned()],
        }
    );
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
