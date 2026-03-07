use super::{
    create_workspace_dir, run_builtin_err, run_builtin_ok, temp_workspace, write_manifest, Path,
    PathBuf, RunnerError,
};

pub(in crate::runner::tests::run_array_tests) fn run_validate_ok(
    root: &Path,
    args: &[&str],
) -> String {
    run_builtin_ok(root.to_path_buf(), "validate", args)
}

pub(in crate::runner::tests::run_array_tests) fn run_validate_err(
    root: &Path,
    args: &[&str],
) -> RunnerError {
    run_builtin_err(root.to_path_buf(), "validate", args)
}

pub(in crate::runner::tests::run_array_tests) fn write_validate_manifest(root: &Path, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn render_builtin_test_suite_run(marker: &Path) -> String {
    format!("sh -lc 'printf called > \\\"{}\\\"'", marker.display())
}

pub(in crate::runner::tests::run_array_tests) fn write_validate_manifest_template(
    root: &Path,
    template: &str,
    replacements: &[(&str, &Path)],
) {
    let mut manifest = template.to_owned();
    for (token, path) in replacements {
        manifest = manifest.replace(token, &path.display().to_string());
    }
    write_validate_manifest(root, &manifest);
}

pub(in crate::runner::tests::run_array_tests) fn write_capture_task_ref_validate_manifest(
    root: &Path,
    marker: &Path,
    capture_run_template: &str,
    capture_env: Option<&str>,
    task_ref_expr: &str,
) {
    let capture_run = capture_run_template.replace("__MARKER__", &marker.display().to_string());
    let capture_run = capture_run.replace("{{args}}", "{args}");
    let capture_run = capture_run.replace('"', "\\\"");
    let mut manifest = format!("[tasks.capture]\nrun = \"{capture_run}\"\n");
    if let Some(env) = capture_env {
        let env = env.replace("{{project}}", "{project}");
        manifest.push_str(&format!("env = {{ {env} }}\n"));
    }
    manifest.push_str(&format!(
        "\n[tasks.validate]\nrun = [{{ task = {task_ref_expr} }}]\n"
    ));
    write_validate_manifest(root, &manifest);
}

pub(in crate::runner::tests::run_array_tests) fn write_validate_builtin_test_task_ref_manifest(
    root: &Path,
    suite_name: &str,
    task_ref: &str,
    marker: &Path,
) {
    let suite_run = render_builtin_test_suite_run(marker);
    write_validate_manifest(
        root,
        &format!(
            "[test.suites]\n{suite_name} = \"{suite_run}\"\n\n[tasks.validate]\nrun = [{{ task = \"{task_ref}\" }}, \"printf validate-ok\"]\n",
        ),
    );
}

pub(in crate::runner::tests::run_array_tests) fn write_catalog_builtin_test_suite_manifest(
    root: &Path,
    catalog_dir: &str,
    alias: &str,
    suite_name: &str,
    marker: &Path,
) {
    let suite_run = render_builtin_test_suite_run(marker);
    let catalog = create_workspace_dir(root, catalog_dir);
    write_manifest(
        &catalog.join("effigy.toml"),
        &format!("[catalog]\nalias = \"{alias}\"\n[test.suites]\n{suite_name} = \"{suite_run}\"\n",),
    );
}

pub(super) fn workspace_with_validate_manifest(workspace: &str, manifest: &str) -> PathBuf {
    let root = temp_workspace(workspace);
    write_validate_manifest(&root, manifest);
    root
}

pub(super) fn workspace_with_marker(
    workspace: &str,
    marker_rel: &str,
    setup: fn(&Path, &Path),
) -> (PathBuf, PathBuf) {
    let root = temp_workspace(workspace);
    let marker = root.join(marker_rel);
    setup(&root, &marker);
    (root, marker)
}
