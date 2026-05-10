use crate::runner::tests::prelude::{
    assert_builtin_help_json_contract_case_table, builtin_help_json_case,
};

#[test]
fn run_manifest_task_builtin_help_json_contract_table_has_stable_schema_topic_and_precedence() {
    let cases = [
        builtin_help_json_case(
            "builtin-cache-help-json-contract",
            "cache",
            &["--help", "--json", "--wat"],
            "tasks-cache",
            "effigy tasks cache inspect [<selector>] [--json]",
        ),
        builtin_help_json_case(
            "builtin-completion-help-json-contract",
            "completion",
            &["--help", "--json", "--wat"],
            "completion",
            "effigy config completion [<bash|zsh|fish>] [--install|--export] [--json]",
        ),
        builtin_help_json_case(
            "builtin-config-help-json-contract",
            "config",
            &["--help", "--json", "--wat"],
            "config",
            "effigy.toml Reference",
        ),
        builtin_help_json_case(
            "builtin-init-help-json-contract",
            "init",
            &["--help", "--json", "--wat"],
            "init",
            "effigy init [<name>] [--dry-run] [--force] [--json]",
        ),
        builtin_help_json_case(
            "builtin-watch-help-json-contract",
            "watch",
            &["--help", "--json", "--wat"],
            "watch",
            "effigy watch --owner <effigy|external>",
        ),
        builtin_help_json_case(
            "builtin-migrate-help-json-contract",
            "migrate",
            &["--help", "--json", "--wat"],
            "tasks-migrate",
            "effigy tasks migrate [--from <PATH>]",
        ),
        builtin_help_json_case(
            "builtin-doctor-help-json-contract",
            "doctor",
            &["--help", "--json", "--wat"],
            "doctor",
            "effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]",
        ),
        builtin_help_json_case(
            "builtin-scan-help-json-contract",
            "scan",
            &["--help", "--json", "--wat"],
            "scan",
            "effigy scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
        ),
        builtin_help_json_case(
            "builtin-tasks-help-json-contract",
            "tasks",
            &["--help", "--json", "--wat"],
            "tasks",
            "effigy tasks [--repo <PATH>] [--task <TASK_NAME>]",
        ),
        builtin_help_json_case(
            "builtin-unlock-help-json-contract",
            "unlock",
            &["--help", "--json", "--wat"],
            "tasks-unlock",
            "effigy tasks unlock [--all | <scope>...] [--yes] [--json]",
        ),
    ];

    assert_builtin_help_json_contract_case_table(&cases);
}
