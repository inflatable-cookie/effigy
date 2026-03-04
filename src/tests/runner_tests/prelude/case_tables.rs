use super::*;

pub(in crate::runner::tests) struct BuiltinErrorCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinHelpCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinArgumentContractCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expect_error: bool,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinArgumentContractCommandCase<'a> {
    pub(in crate::runner::tests) command: &'a str,
    pub(in crate::runner::tests) cases: &'a [BuiltinArgumentContractCase],
}

pub(in crate::runner::tests) struct BuiltinInvocationCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) struct BuiltinInvocationSetupCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected: &'static [&'static str],
    pub(in crate::runner::tests) setup: fn(&Path),
}

pub(in crate::runner::tests) struct BuiltinHelpJsonCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected_topic: &'static str,
    pub(in crate::runner::tests) expected_usage_fragment: &'static str,
}

pub(in crate::runner::tests) struct ManifestParseRejectionCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

pub(in crate::runner::tests) fn assert_builtin_help_case(case: &BuiltinHelpCase) {
    let root = temp_workspace(case.workspace);
    write_root_manifest(&root, "");
    let out = run_builtin_ok(root, case.command, case.args);
    assert_contains_all(&out, case.expected);
}

pub(in crate::runner::tests) fn assert_builtin_help_case_table(cases: &[BuiltinHelpCase]) {
    assert_case_table(cases.iter(), |case| {
        assert_builtin_help_case(case);
    });
}

pub(in crate::runner::tests) fn assert_case_table<T>(
    cases: impl IntoIterator<Item = T>,
    mut assert_case: impl FnMut(T),
) {
    for case in cases {
        assert_case(case);
    }
}

pub(in crate::runner::tests) fn assert_builtin_error_case(case: &BuiltinErrorCase) {
    let root = temp_workspace(case.workspace);
    write_root_manifest(&root, case.manifest);
    let err = run_builtin_err(root, case.command, case.args);
    assert_task_invocation_error_contains(err, case.expected);
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table(cases: &[BuiltinErrorCase]) {
    assert_case_table(cases.iter(), |case| {
        assert_builtin_error_case(case);
    });
}

pub(in crate::runner::tests) fn assert_builtin_argument_contract_case_table(
    command: &str,
    cases: &[BuiltinArgumentContractCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        write_root_manifest(&root, "");
        if case.expect_error {
            let err = run_builtin_err(root, command, case.args);
            assert_task_invocation_error_contains(err, case.expected);
        } else {
            let out = run_builtin_ok(root, command, case.args);
            assert_contains_all(&out, case.expected);
        }
    });
}

pub(in crate::runner::tests) fn assert_builtin_argument_contract_command_case_table(
    command_cases: &[BuiltinArgumentContractCommandCase<'_>],
) {
    assert_case_table(command_cases.iter(), |case| {
        assert_builtin_argument_contract_case_table(case.command, case.cases);
    });
}

pub(in crate::runner::tests) fn assert_builtin_ok_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&Path),
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        setup(&root);
        let out = run_builtin_ok(root, command, case.args);
        assert_contains_all(&out, case.expected);
    });
}

pub(in crate::runner::tests) fn assert_builtin_ok_case_table_with_case_setup(
    command: &str,
    cases: &[BuiltinInvocationSetupCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        (case.setup)(&root);
        let out = run_builtin_ok(root, command, case.args);
        assert_contains_all(&out, case.expected);
    });
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table_with_setup(
    command: &str,
    cases: &[BuiltinInvocationCase],
    setup: impl Fn(&Path),
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        setup(&root);
        let err = run_builtin_err(root, command, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    });
}

pub(in crate::runner::tests) fn assert_builtin_error_case_table_with_case_setup(
    command: &str,
    cases: &[BuiltinInvocationSetupCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        (case.setup)(&root);
        let err = run_builtin_err(root, command, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    });
}

pub(in crate::runner::tests) fn assert_builtin_help_json_contract_case_table(
    cases: &[BuiltinHelpJsonCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        write_root_manifest(&root, "");

        let out = run_builtin_ok(root, case.command, case.args);
        let parsed = parse_json_output(&out);
        assert_eq!(parsed["schema"], "effigy.help.v1");
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["topic"], case.expected_topic);
        let text = parsed["text"].as_str().expect("help text string");
        assert!(
            text.contains(case.expected_usage_fragment),
            "expected help text for {} to include {:?}, got:\n{}",
            case.command,
            case.expected_usage_fragment,
            text
        );
    });
}

pub(in crate::runner::tests) fn assert_tasks_manifest_parse_rejection_case_table(
    cases: &[ManifestParseRejectionCase],
) {
    assert_case_table(cases.iter(), |case| {
        let root = temp_workspace(case.workspace);
        write_manifest(&root.join("effigy.toml"), case.manifest);
        let err = run_tasks_with_repo(root).expect_err("expected manifest parse failure");
        assert_task_manifest_parse_runner_error_contains_any(err, case.expected);
    });
}

pub(in crate::runner::tests) fn string_args(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| (*arg).to_owned()).collect()
}

pub(in crate::runner::tests) fn assert_parser_task_invocation_error<T>(
    result: Result<T, RunnerError>,
    expected: &str,
) {
    match result {
        Ok(_) => panic!("expected parser task invocation error containing `{expected}`"),
        Err(err) => assert_task_invocation_error_contains(err, &[expected]),
    }
}
