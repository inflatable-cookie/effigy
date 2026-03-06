pub(super) use super::super::prelude::{
    assert_case_table, assert_file_text_equals, assert_invocation_error_contains,
    assert_output_contains_all, assert_output_contains_any, assert_output_equals,
    assert_path_exists, assert_run_step_task_ref_parse_error_contains, create_workspace_dir, fs,
    run_builtin_err, run_builtin_ok, temp_workspace, write_manifest, Path, PathBuf, RunnerError,
};

pub(super) fn run_validate_ok(root: &Path, args: &[&str]) -> String {
    run_builtin_ok(root.to_path_buf(), "validate", args)
}

pub(super) fn run_validate_err(root: &Path, args: &[&str]) -> RunnerError {
    run_builtin_err(root.to_path_buf(), "validate", args)
}

pub(super) struct RunArrayInvocationErrorCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) expected: &'static [&'static str],
}

pub(super) struct RunArrayValidateOutputCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected: &'static [&'static str],
}

pub(super) struct RunArrayTaskOutputCase {
    pub(super) workspace: &'static str,
    pub(super) task: &'static str,
    pub(super) marker_rel: &'static str,
    pub(super) expected: &'static str,
    pub(super) setup: fn(&Path, &Path),
}

pub(super) struct RunArrayTaskOutputDerivedCase {
    pub(super) workspace: &'static str,
    pub(super) task: &'static str,
    pub(super) marker_rel: &'static str,
    pub(super) expected: fn(&Path) -> String,
    pub(super) setup: fn(&Path, &Path),
}

pub(super) struct RunArrayTaskInvocationErrorCase {
    pub(super) workspace: &'static str,
    pub(super) task: &'static str,
    pub(super) expected: &'static [&'static str],
    pub(super) setup: fn(&Path),
}

pub(super) struct RunArrayTaskRefParseErrorCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) expected_tail: &'static str,
}

pub(super) struct RunArrayInvocationMessageCase {
    pub(super) workspace: &'static str,
    pub(super) manifest: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) expected_all: &'static [&'static str],
    pub(super) expected_any: &'static [&'static str],
    pub(super) expected_exact: Option<&'static str>,
}

pub(super) struct BuiltinTestTaskRefCase {
    pub(super) workspace: &'static str,
    pub(super) suite_name: &'static str,
    pub(super) task_ref: &'static str,
}

pub(super) struct RunArrayValidateMarkerCase {
    pub(super) workspace: &'static str,
    pub(super) args: &'static [&'static str],
    pub(super) marker_rel: &'static str,
    pub(super) expected: &'static [&'static str],
    pub(super) setup: fn(&Path, &Path),
}

pub(super) fn write_validate_manifest(root: &Path, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn workspace_with_validate_manifest(workspace: &str, manifest: &str) -> PathBuf {
    let root = temp_workspace(workspace);
    write_validate_manifest(&root, manifest);
    root
}

fn workspace_with_marker(
    workspace: &str,
    marker_rel: &str,
    setup: fn(&Path, &Path),
) -> (PathBuf, PathBuf) {
    let root = temp_workspace(workspace);
    let marker = root.join(marker_rel);
    setup(&root, &marker);
    (root, marker)
}

fn for_each_manifest_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    manifest_of: impl Fn(&T) -> &str,
    mut assert_case: impl FnMut(&T, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let root = workspace_with_validate_manifest(workspace_of(case), manifest_of(case));
        assert_case(case, root);
    });
}

fn for_each_marker_case<T>(
    cases: &[T],
    workspace_of: impl Fn(&T) -> &str,
    marker_rel_of: impl Fn(&T) -> &str,
    setup_of: impl Fn(&T) -> fn(&Path, &Path),
    mut assert_case: impl FnMut(&T, PathBuf, PathBuf),
) {
    assert_case_table(cases.iter(), |case| {
        let (root, marker) =
            workspace_with_marker(workspace_of(case), marker_rel_of(case), setup_of(case));
        assert_case(case, root, marker);
    });
}

pub(super) fn write_validate_manifest_template(
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

pub(super) fn run_validate_err_invocation_message(root: &Path, args: &[&str]) -> String {
    match run_validate_err(root, args) {
        RunnerError::TaskInvocation(message) => message,
        other => panic!("unexpected error: {other}"),
    }
}

fn render_builtin_test_suite_run(marker: &Path) -> String {
    format!("sh -lc 'printf called > \\\"{}\\\"'", marker.display())
}

pub(super) fn write_capture_task_ref_validate_manifest(
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

pub(super) fn write_validate_builtin_test_task_ref_manifest(
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

pub(super) fn write_catalog_builtin_test_suite_manifest(
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

pub(super) fn assert_run_array_validate_output_case_table(cases: &[RunArrayValidateOutputCase]) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |case| case.manifest,
        |case, root| {
            let out = run_validate_ok(&root, case.args);
            assert_output_contains_all(&out, case.expected);
        },
    );
}

pub(super) fn assert_task_output_equals(root: &Path, task: &str, marker: &Path, expected: &str) {
    let out = run_builtin_ok(root.to_path_buf(), task, &[]);
    assert_output_equals(&out, "");
    assert_file_text_equals(marker, expected);
}

pub(super) fn assert_run_array_task_output_case_table(cases: &[RunArrayTaskOutputCase]) {
    for_each_marker_case(
        cases,
        |case| case.workspace,
        |case| case.marker_rel,
        |case| case.setup,
        |case, root, marker| {
            assert_task_output_equals(&root, case.task, &marker, case.expected);
        },
    );
}

pub(super) fn assert_run_array_task_output_derived_case_table(
    cases: &[RunArrayTaskOutputDerivedCase],
) {
    for_each_marker_case(
        cases,
        |case| case.workspace,
        |case| case.marker_rel,
        |case| case.setup,
        |case, root, marker| {
            let expected = (case.expected)(&root);
            assert_task_output_equals(&root, case.task, &marker, &expected);
        },
    );
}

pub(super) fn assert_run_array_task_invocation_error_case_table(
    cases: &[RunArrayTaskInvocationErrorCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        (case.setup)(&root);
        let err = run_builtin_err(root, case.task, &[]);
        assert_invocation_error_contains(err, case.expected);
    });
}

pub(super) fn assert_run_array_validate_invocation_error_case_table(
    cases: &[RunArrayInvocationErrorCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |case| case.manifest,
        |case, root| {
            let err = run_validate_err(&root, &[]);
            assert_invocation_error_contains(err, case.expected);
        },
    );
}

pub(super) fn assert_run_array_validate_task_ref_parse_error_case_table(
    cases: &[RunArrayTaskRefParseErrorCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |case| case.manifest,
        |case, root| {
            let err = run_validate_err(&root, &[]);
            assert_run_step_task_ref_parse_error_contains(err, case.expected_tail);
        },
    );
}

pub(super) fn assert_run_array_validate_invocation_message_case_table(
    cases: &[RunArrayInvocationMessageCase],
) {
    for_each_manifest_case(
        cases,
        |case| case.workspace,
        |case| case.manifest,
        |case, root| {
            let message = run_validate_err_invocation_message(&root, case.args);
            if let Some(expected_exact) = case.expected_exact {
                assert_output_equals(&message, expected_exact);
            } else {
                assert_output_contains_all(&message, case.expected_all);
                if !case.expected_any.is_empty() {
                    assert_output_contains_any(&message, case.expected_any);
                }
            }
        },
    );
}

pub(super) fn assert_run_array_builtin_test_task_ref_case_table(cases: &[BuiltinTestTaskRefCase]) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        let marker = root.join("builtin-test-called.log");
        write_validate_builtin_test_task_ref_manifest(
            &root,
            case.suite_name,
            case.task_ref,
            &marker,
        );

        let out = run_validate_ok(&root, &["--verbose-root"]);
        assert_output_contains_all(&out, &["validate-ok"]);
        assert_path_exists(&marker, "built-in test task ref marker");
    });
}

pub(super) fn assert_run_array_validate_marker_case_table(cases: &[RunArrayValidateMarkerCase]) {
    for_each_marker_case(
        cases,
        |case| case.workspace,
        |case| case.marker_rel,
        |case| case.setup,
        |case, root, marker| {
            let out = run_validate_ok(&root, case.args);
            assert_output_contains_all(&out, case.expected);
            assert_path_exists(&marker, "validate marker");
        },
    );
}
