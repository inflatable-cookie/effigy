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

pub(in crate::runner::tests) fn builtin_help_case(
    workspace: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
) -> BuiltinHelpCase {
    BuiltinHelpCase {
        workspace,
        command,
        args,
        expected,
    }
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

pub(in crate::runner::tests) fn builtin_invocation_case(
    workspace: &'static str,
    args: &'static [&'static str],
    expected: &'static [&'static str],
) -> BuiltinInvocationCase {
    BuiltinInvocationCase {
        workspace,
        args,
        expected,
    }
}

pub(in crate::runner::tests) struct BuiltinHelpJsonCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) command: &'static str,
    pub(in crate::runner::tests) args: &'static [&'static str],
    pub(in crate::runner::tests) expected_topic: &'static str,
    pub(in crate::runner::tests) expected_usage_fragment: &'static str,
}

pub(in crate::runner::tests) fn builtin_help_json_case(
    workspace: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    expected_topic: &'static str,
    expected_usage_fragment: &'static str,
) -> BuiltinHelpJsonCase {
    BuiltinHelpJsonCase {
        workspace,
        command,
        args,
        expected_topic,
        expected_usage_fragment,
    }
}

pub(in crate::runner::tests) struct ManifestParseRejectionCase {
    pub(in crate::runner::tests) workspace: &'static str,
    pub(in crate::runner::tests) manifest: &'static str,
    pub(in crate::runner::tests) expected: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub(super) enum BuiltinInvocationOutcome {
    Success,
    Error,
}
