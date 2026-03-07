use super::prelude::{
    assert_builtin_error_contract_case_table, assert_builtin_help_case_table, builtin_help_case,
    builtin_shared_unknown_argument_cases,
};

#[test]
fn run_manifest_task_builtin_entrypoint_help_json_contract_table() {
    let cases = [
        builtin_help_case(
            "builtin-entrypoint-cache-help-json",
            "cache",
            &["--wat", "--help", "--json"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"cache\""],
        ),
        builtin_help_case(
            "builtin-entrypoint-completion-help-json",
            "completion",
            &["--help", "--json", "--wat"],
            &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion\"",
            ],
        ),
        builtin_help_case(
            "builtin-entrypoint-completion-candidates-help-json",
            "completion",
            &["candidates", "--help", "--json", "--wat"],
            &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion-candidates\"",
            ],
        ),
        builtin_help_case(
            "builtin-entrypoint-watch-help-json",
            "watch",
            &["--help", "--json", "--wat"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"watch\""],
        ),
        builtin_help_case(
            "builtin-entrypoint-scan-help-json",
            "scan",
            &["god-files", "--help", "--json", "--wat"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"scan\""],
        ),
        builtin_help_case(
            "builtin-entrypoint-scan-bare-help-json",
            "scan",
            &["--json"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"scan\""],
        ),
        builtin_help_case(
            "builtin-entrypoint-unlock-help-json",
            "unlock",
            &["--help", "--json", "--wat"],
            &["\"schema\": \"effigy.help.v1\"", "\"topic\": \"unlock\""],
        ),
    ];

    assert_builtin_help_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_entrypoint_unknown_argument_contract_table() {
    let cases = builtin_shared_unknown_argument_cases(
        "builtin-entrypoint-cache-unknown-arg",
        "builtin-entrypoint-completion-candidates-unknown-arg",
        "builtin-entrypoint-watch-unknown-arg",
        "builtin-entrypoint-scan-unknown-arg",
        "builtin-entrypoint-unlock-unknown-arg",
    );

    assert_builtin_error_contract_case_table(&cases);
}
