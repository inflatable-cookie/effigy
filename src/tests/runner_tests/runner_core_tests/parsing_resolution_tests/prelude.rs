pub(super) use super::super::prelude::{
    assert_builtin_error_case_table, assert_output_contains_all, assert_run_task_ok_empty,
    builtin_test_max_parallel, discover_catalogs, fs, lock_test, parse_task_runtime_args,
    parse_task_selector, run_builtin_ok, run_task, temp_workspace, write_executable,
    write_manifest, write_root_manifest, BuiltinErrorCase, EnvGuard, PathBuf, RunnerError,
    TaskRuntimeArgs,
};

pub(super) fn assert_catalog_prefix_not_found(
    err: RunnerError,
    expected_prefix: &str,
    expected_available: &[&str],
) {
    match err {
        RunnerError::TaskCatalogPrefixNotFound { prefix, available } => {
            assert_eq!(prefix, expected_prefix);
            assert_eq!(
                available,
                expected_available
                    .iter()
                    .map(|value| (*value).to_owned())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("unexpected error: {other}"),
    }
}
