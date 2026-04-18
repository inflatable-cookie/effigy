use super::super::harness::{
    create_workspace_dir, write_catalog_tasks, write_manifest, write_root_manifest, EnvGuard,
};
use super::super::runtime::Path;

pub(in crate::runner::tests) fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

pub(in crate::runner::tests) fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

pub(in crate::runner::tests) fn write_catalogs_with_tasks(
    root: &Path,
    catalogs: &[(&str, &[(&str, &str)])],
) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

pub(in crate::runner::tests) fn write_catalog_a_and_catalog_c_dev_catalogs(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            ("catalog_a", &[("api", "printf catalog_a-api")]),
            ("catalog_c", &[("dev", "printf catalog_c-dev")]),
        ],
    );
}

pub(in crate::runner::tests) fn write_froyo_validate_catalog(root: &Path) {
    write_catalogs_with_tasks(root, &[("froyo", &[("validate", "printf froyo-validate")])]);
}

pub(in crate::runner::tests) fn write_managed_admin_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "front", run = "vite dev", start = 2, tab = 2 },
  { name = "admin", run = "vite dev --config admin", start = 3, tab = 3 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "admin", run = "vite dev --config admin", start = 2, tab = 2 }
]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_stream_builtin_test_manifest(
    root: &Path,
    suite: &str,
    test_task_ref: &str,
    marker: &Path,
) {
    write_root_manifest(
        root,
        &format!(
            r#"[test.suites]
{} = "sh -lc 'printf called > \"{}\"'"

[tasks.dev]
mode = "tui"
concurrent = [{{ name = "tests", task = "{}" }}]
"#,
            suite,
            marker.display(),
            test_task_ref
        ),
    );
}

pub(in crate::runner::tests) fn write_managed_stream_profile_manifest(root: &Path) {
    write_root_manifest(
        root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "default-only", run = "printf default-ok" }]

[tasks.dev.profiles.front]
concurrent = [{ name = "front-only", run = "printf front-ok" }]
"#,
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest(root: &Path, concurrent: &str) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n"),
    );
}

pub(in crate::runner::tests) fn write_managed_tui_dev_manifest_with_extra(
    root: &Path,
    concurrent: &str,
    extra_sections: &str,
) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n\n{extra_sections}\n"),
    );
}

pub(in crate::runner::tests) fn write_catalog_manifest_with_alias(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    body: &str,
) {
    let dir = create_workspace_dir(root, catalog_dir);
    write_manifest(
        &dir.join("effigy.toml"),
        &format!("[catalog]\nalias = \"{alias}\"\n{body}\n"),
    );
}

pub(in crate::runner::tests) fn write_ranked_task_ref_manifest(
    root: &Path,
    jobs_start_after_ms: Option<u32>,
) {
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

pub(in crate::runner::tests) fn write_ranked_name_manifest(root: &Path) {
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

pub(in crate::runner::tests) fn write_ranked_catalog_tasks(root: &Path) {
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
            (
                "catalog_c",
                &[("dev", "printf catalog_c-dev")] as &[(&str, &str)],
            ),
            (
                "catalog_b",
                &[("dev", "printf catalog_b-dev")] as &[(&str, &str)],
            ),
        ],
    );
}
