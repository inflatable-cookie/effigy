pub(super) use super::super::super::prelude::{
    assert_managed_invalid_definition_case_table, assert_managed_output_case_table, lock_test,
    managed_tui_env, write_catalogs_with_tasks, write_root_manifest, ManagedInvalidDefinitionCase,
    ManagedInvocation, ManagedOutputCase, Path,
};

pub(super) fn write_ranked_task_ref_manifest(root: &Path, jobs_start_after_ms: Option<u32>) {
    let jobs_delay = jobs_start_after_ms
        .map(|ms| format!(", start_after_ms = {ms}"))
        .unwrap_or_default();
    write_root_manifest(
        root,
        &format!(
            r#"[tasks.dev]
mode = "tui"
concurrent = [
  {{ task = "catalog_a/api", start = 1, tab = 3 }},
  {{ task = "catalog_a/jobs", start = 2, tab = 4{} }},
  {{ task = "catalog_c/dev", start = 3, tab = 2 }},
  {{ task = "catalog_b/dev", start = 4, tab = 1 }}
]
"#,
            jobs_delay
        ),
    );
}

pub(super) fn write_ranked_name_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "printf api", start = 1, tab = 3 },
  { name = "jobs", run = "printf jobs", start = 2, tab = 4 },
  { name = "catalog_c", run = "printf catalog_c", start = 3, tab = 2 },
  { name = "catalog_b", run = "printf catalog_b", start = 4, tab = 1 }
]
"#,
    );
}

pub(super) fn write_ranked_catalog_tasks(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            (
                "catalog_a",
                &[
                    ("api", "printf catalog_a-api"),
                    ("jobs", "printf catalog_a-jobs"),
                ] as &[(&str, &str)],
            ),
            ("catalog_c", &[("dev", "printf catalog_c-dev")] as &[(&str, &str)]),
            ("catalog_b", &[("dev", "printf catalog_b-dev")] as &[(&str, &str)]),
        ],
    );
}
