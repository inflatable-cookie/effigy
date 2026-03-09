pub(super) mod cases {
    pub(in crate::runner::tests) use super::super::super::prelude::cases::*;
}

pub(super) mod harness {
    pub(in crate::runner::tests) use super::super::super::prelude::harness_assertions::*;
    pub(in crate::runner::tests) use super::super::super::prelude::harness_builtin::*;
    pub(in crate::runner::tests) use super::super::super::prelude::harness_env::*;
    pub(in crate::runner::tests) use super::super::super::prelude::harness_workspace::*;
}

pub(super) mod json {
    pub(in crate::runner::tests) use super::super::super::prelude::json::*;
}

pub(super) mod output {
    pub(in crate::runner::tests) use super::super::super::prelude::output::*;
}

pub(super) mod runtime {
    pub(in crate::runner::tests) use super::super::super::prelude::runtime::*;
}

pub(super) use super::super::prelude::harness_builtin::*;
pub(super) use super::super::prelude::harness_env::*;
pub(super) use super::super::prelude::harness_workspace::*;
pub(super) use super::super::prelude::json::*;
pub(super) use super::super::prelude::output::*;
pub(super) use super::super::prelude::runtime::*;

pub(super) fn setup_fanout_catalog_repo(root: &Path) -> (PathBuf, PathBuf) {
    let catalog_a = root.join("catalog_a");
    let catalog_b = root.join("catalog_b");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    fs::create_dir_all(&catalog_b).expect("mkdir catalog_b");
    write_root_manifest(root, "[tasks.dev]\nrun = \"printf root\"\n");
    write_manifest(
        &catalog_a.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_a\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_manifest(
        &catalog_b.join("effigy.toml"),
        "[catalog]\nalias = \"catalog_b\"\n[tasks.ping]\nrun = \"printf ok\"\n",
    );
    write_package_json_with_test_script(&catalog_a);
    write_package_json_with_test_script(&catalog_b);
    (catalog_a, catalog_b)
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
