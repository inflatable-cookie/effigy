pub(super) use super::super::prelude::*;

pub(super) fn setup_fanout_catalog_repo(root: &Path) -> (PathBuf, PathBuf) {
    let farmyard = root.join("farmyard");
    let dairy = root.join("dairy");
    fs::create_dir_all(&farmyard).expect("mkdir farmyard");
    fs::create_dir_all(&dairy).expect("mkdir dairy");
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &farmyard.join("effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &dairy.join("effigy.toml"),
        "[catalog]\nalias = \"dairy\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_package_json_with_test_script(&farmyard);
    write_package_json_with_test_script(&dairy);
    (farmyard, dairy)
}

pub(super) fn assert_builtin_test_non_zero(
    err: RunnerError,
    expected_failures: Option<Vec<(String, Option<i32>)>>,
    expected_rendered_snippets: &[&str],
    unexpected_rendered_snippets: &[&str],
) {
    match err {
        RunnerError::BuiltinTestNonZero { failures, rendered } => {
            if let Some(expected) = expected_failures {
                assert_eq!(failures, expected);
            }
            assert_output_contains_all(&rendered, expected_rendered_snippets);
            assert_output_excludes_all(&rendered, unexpected_rendered_snippets);
        }
        other => panic!("unexpected error: {other}"),
    }
}
