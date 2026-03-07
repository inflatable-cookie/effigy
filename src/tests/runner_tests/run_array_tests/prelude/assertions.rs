use super::{
    assert_case_table, assert_file_text_equals, assert_invocation_error_contains,
    assert_output_contains_all, assert_output_contains_any, assert_output_equals,
    assert_path_exists, assert_run_step_task_ref_parse_error_contains, run_builtin_err,
    run_builtin_ok, run_validate_err, run_validate_err_invocation_message, run_validate_ok,
    temp_workspace, workspace_with_marker, workspace_with_validate_manifest,
    write_validate_builtin_test_task_ref_manifest, BuiltinTestTaskRefCase, Path, PathBuf,
    RunArrayInvocationErrorCase, RunArrayInvocationMessageCase, RunArrayTaskInvocationErrorCase,
    RunArrayTaskOutputCase, RunArrayTaskOutputDerivedCase, RunArrayTaskRefParseErrorCase,
    RunArrayValidateMarkerCase, RunArrayValidateOutputCase,
};

pub(in crate::runner::tests::run_array_tests) fn assert_task_output_equals(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_validate_output_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_task_output_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_task_output_derived_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_task_invocation_error_case_table(
    cases: &[RunArrayTaskInvocationErrorCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        (case.setup)(&root);
        let err = run_builtin_err(root, case.task, &[]);
        assert_invocation_error_contains(err, case.expected);
    });
}

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_validate_invocation_error_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_validate_task_ref_parse_error_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_validate_invocation_message_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_builtin_test_task_ref_case_table(
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

pub(in crate::runner::tests::run_array_tests) fn assert_run_array_validate_marker_case_table(
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
