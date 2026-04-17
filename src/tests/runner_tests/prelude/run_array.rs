use super::cases::assert_case_table;
use super::errors::{
    assert_invocation_error_contains, assert_run_step_task_ref_parse_error_contains,
};
use super::harness::{
    create_workspace_dir, run_builtin_err, run_builtin_ok, temp_workspace, write_manifest,
};
use super::output::{
    assert_file_text_equals, assert_output_contains_all, assert_output_contains_any,
    assert_output_equals, assert_path_exists,
};
use super::runtime::{Path, PathBuf, RunnerError};

// ---------------------------------------------------------------------------
// Cases
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) struct RunArrayInvocationErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct RunArrayValidateOutputCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct RunArrayTaskOutputCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) task: &'static str,
    pub(in crate::runner::tests) marker_rel: &'static str,
    pub(in crate::runner::tests) expected: &'static str,
    pub(in crate::runner::tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests) struct RunArrayTaskOutputDerivedCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) task: &'static str,
    pub(in crate::runner::tests) marker_rel: &'static str,
    pub(in crate::runner::tests) expected: fn(&Path) -> String,
    pub(in crate::runner::tests) setup: fn(&Path, &Path),
}

pub(in crate::runner::tests) struct RunArrayTaskInvocationErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) task: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct RunArrayTaskRefParseErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected_tail: &'static str,
}

pub(in crate::runner::tests) struct RunArrayInvocationMessageCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected_all: &'static [&'static str],
    pub(in crate::runner::tests) expected_any: &'static [&'static str],
    pub(in crate::runner::tests) expected_exact: Option<&'static str>,
}

pub(in crate::runner::tests) struct BuiltinTestTaskRefCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) suite_name: &'static str,
    pub(in crate::runner::tests) task_ref: &'static str,
}

pub(in crate::runner::tests) struct RunArrayValidateMarkerCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) marker_rel: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path, &Path),
}

// ---------------------------------------------------------------------------
// Workspace helpers
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) fn run_validate_ok(root: &Path, args: &[&str]) -> String {
    run_builtin_ok(root.to_path_buf(), "validate", args)
}

pub(in crate::runner::tests) fn run_validate_err(root: &Path, args: &[&str]) -> RunnerError {
    run_builtin_err(root.to_path_buf(), "validate", args)
}

pub(in crate::runner::tests) fn write_validate_manifest(root: &Path, body: &str) {
    write_manifest(&root.join("effigy.toml"), body);
}

fn render_builtin_test_suite_run(marker: &Path) -> String {
    format!("sh -lc 'printf called > \\\"{}\\\"'", marker.display())
}

pub(in crate::runner::tests) fn write_validate_manifest_template(
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

pub(in crate::runner::tests) fn write_capture_task_ref_validate_manifest(
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

pub(in crate::runner::tests) fn write_validate_builtin_test_task_ref_manifest(
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

pub(in crate::runner::tests) fn write_catalog_builtin_test_suite_manifest(
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

pub(in crate::runner::tests) fn run_validate_err_invocation_message(
    root: &Path,
    args: &[&str],
) -> String {
    match run_validate_err(root, args) {
        RunnerError::TaskInvocation(message) => message,
        other => panic!("unexpected error: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Assertions / case-table runners
// ---------------------------------------------------------------------------

pub(in crate::runner::tests) fn assert_task_output_equals(
    root: &Path,
    task: &str,
    marker: &Path,
    expected: &str,
) {
    let out = run_builtin_ok(root.to_path_buf(), task, &[]);
    assert_output_equals(&out, "");
    assert_file_text_equals(marker, expected);
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

pub(in crate::runner::tests) fn assert_run_array_validate_output_case_table(
    cases: &[RunArrayValidateOutputCase],
) {
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

pub(in crate::runner::tests) fn assert_run_array_task_output_case_table(
    cases: &[RunArrayTaskOutputCase],
) {
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

pub(in crate::runner::tests) fn assert_run_array_task_output_derived_case_table(
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

pub(in crate::runner::tests) fn assert_run_array_task_invocation_error_case_table(
    cases: &[RunArrayTaskInvocationErrorCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        (case.setup)(&root);
        let err = run_builtin_err(root, case.task, &[]);
        assert_invocation_error_contains(err, case.expected);
    });
}

pub(in crate::runner::tests) fn assert_run_array_validate_invocation_error_case_table(
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

pub(in crate::runner::tests) fn assert_run_array_validate_task_ref_parse_error_case_table(
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

pub(in crate::runner::tests) fn assert_run_array_validate_invocation_message_case_table(
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

pub(in crate::runner::tests) fn assert_run_array_builtin_test_task_ref_case_table(
    cases: &[BuiltinTestTaskRefCase],
) {
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

pub(in crate::runner::tests) fn assert_run_array_validate_marker_case_table(
    cases: &[RunArrayValidateMarkerCase],
) {
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
