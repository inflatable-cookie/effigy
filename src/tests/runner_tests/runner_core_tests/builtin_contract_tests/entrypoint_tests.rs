use crate::runner::tests::prelude::{assert_builtin_help_case_table, builtin_help_case};

#[test]
fn run_manifest_task_builtin_entrypoint_help_json_contract_table() {
    // Completion and completion-candidates `--help --json` payloads are pinned
    // by help_tests/json_contract_tests (parsed topic/usage) and
    // help_tests/precedence_tests (rendered candidates schema/topic); their
    // unknown-argument contracts live in argument_tests' shared cases.
    let cases = [
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
    ];

    assert_builtin_help_case_table(&cases);
}
