use super::{
    create_workspace_dir, write_catalog_tasks, write_manifest, write_root_manifest, EnvGuard, Path,
};

pub(crate) fn managed_tui_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_TUI", Some("0".to_owned()))])
}

pub(crate) fn managed_stream_env() -> EnvGuard {
    EnvGuard::set_many(&[("EFFIGY_MANAGED_STREAM", Some("1".to_owned()))])
}

pub(crate) fn write_catalogs_with_tasks(root: &Path, catalogs: &[(&str, &[(&str, &str)])]) {
    for (name, tasks) in catalogs {
        let dir = create_workspace_dir(root, name);
        write_catalog_tasks(&dir, Some(name), tasks);
    }
}

pub(crate) fn write_catalog_a_and_catalog_c_dev_catalogs(root: &Path) {
    write_catalogs_with_tasks(
        root,
        &[
            ("catalog_a", &[("api", "printf catalog_a-api")]),
            ("catalog_c", &[("dev", "printf catalog_c-dev")]),
        ],
    );
}

pub(crate) fn write_froyo_validate_catalog(root: &Path) {
    write_catalogs_with_tasks(root, &[("froyo", &[("validate", "printf froyo-validate")])]);
}

pub(crate) fn write_managed_admin_profile_manifest(root: &Path) {
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

pub(crate) fn write_managed_stream_builtin_test_manifest(
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

pub(crate) fn write_managed_stream_profile_manifest(root: &Path) {
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

pub(crate) fn write_managed_tui_dev_manifest(root: &Path, concurrent: &str) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n"),
    );
}

pub(crate) fn write_managed_tui_dev_manifest_with_extra(
    root: &Path,
    concurrent: &str,
    extra_sections: &str,
) {
    write_root_manifest(
        root,
        &format!("[tasks.dev]\nmode = \"tui\"\nconcurrent = {concurrent}\n\n{extra_sections}\n"),
    );
}

pub(crate) fn write_catalog_manifest_with_alias(
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
