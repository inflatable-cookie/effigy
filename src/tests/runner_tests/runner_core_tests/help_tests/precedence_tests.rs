use crate::runner::tests::prelude::{
    assert_builtin_help_case_table, builtin_help_case, builtin_scan_help_precedence_cases,
    builtin_shared_help_precedence_cases,
};

#[test]
fn run_manifest_task_builtin_help_precedence_contract_table() {
    let mut cases = builtin_shared_help_precedence_cases(
        "builtin-cache-help-precedence",
        "builtin-completion-help-precedence",
        "builtin-completion-candidates-help-precedence",
    );
    cases.extend([
        builtin_help_case(
            "builtin-completion-candidates-help-json-precedence",
            "completion",
            &["candidates", "--help", "--json", "--wat"],
            &[
                "\"schema\": \"effigy.help.v1\"",
                "\"topic\": \"completion-candidates\"",
            ],
        ),
        builtin_help_case(
            "builtin-config-help-precedence",
            "config",
            &["--help", "--wat"],
            &["effigy.toml Reference", "[tasks]"],
        ),
        builtin_help_case(
            "builtin-unlock-help-precedence",
            "unlock",
            &["--wat", "--help"],
            &["unlock Help", "effigy unlock [--all | <scope>...] [--json]"],
        ),
    ]);
    cases.extend(builtin_scan_help_precedence_cases());

    assert_builtin_help_case_table(&cases);
}
