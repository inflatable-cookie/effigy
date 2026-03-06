use super::prelude::{
    assert_builtin_error_contract_case_table, assert_builtin_help_case_table,
    BuiltinContractErrorCase, BuiltinHelpCase,
};

#[test]
fn run_manifest_task_builtin_entrypoint_help_json_contract_table() {
    let cases = [
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-cache-help-json",
            command: "cache",
            args: &["--wat", "--help", "--json"],
            expected: &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"cache\""],
        },
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-completion-help-json",
            command: "completion",
            args: &["--help", "--json", "--wat"],
            expected: &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion\"",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-completion-candidates-help-json",
            command: "completion",
            args: &["candidates", "--help", "--json", "--wat"],
            expected: &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion-candidates\"",
            ],
        },
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-watch-help-json",
            command: "watch",
            args: &["--help", "--json", "--wat"],
            expected: &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"watch\""],
        },
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-scan-help-json",
            command: "scan",
            args: &["god-files", "--help", "--json", "--wat"],
            expected: &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"scan\""],
        },
        BuiltinHelpCase {
            workspace: "builtin-entrypoint-unlock-help-json",
            command: "unlock",
            args: &["--help", "--json", "--wat"],
            expected: &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"unlock\""],
        },
    ];

    assert_builtin_help_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_entrypoint_unknown_argument_contract_table() {
    let cases = [
        BuiltinContractErrorCase {
            workspace: "builtin-entrypoint-cache-unknown-arg",
            command: "cache",
            args: &["inspect", "--wat"],
            expected: &["unknown argument(s) for built-in `cache`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-entrypoint-completion-candidates-unknown-arg",
            command: "completion",
            args: &["candidates", "--wat"],
            expected: &["unknown argument(s) for built-in `completion`: candidates --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-entrypoint-watch-unknown-arg",
            command: "watch",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `watch`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-entrypoint-scan-unknown-arg",
            command: "scan",
            args: &["god-files", "--wat"],
            expected: &["unknown argument(s) for built-in `scan`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-entrypoint-unlock-unknown-arg",
            command: "unlock",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `unlock`: --wat"],
        },
    ];

    assert_builtin_error_contract_case_table(&cases);
}
