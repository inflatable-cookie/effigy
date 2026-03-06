use super::prelude::{assert_builtin_help_json_contract_case_table, BuiltinHelpJsonCase};

#[test]
fn run_manifest_task_builtin_help_json_contract_table_has_stable_schema_topic_and_precedence() {
    let cases = [
        BuiltinHelpJsonCase {
            workspace: "builtin-cache-help-json-contract",
            command: "cache",
            args: &["--help", "--json", "--wat"],
            expected_topic: "cache",
            expected_usage_fragment: "effigy cache inspect [<selector>] [--json]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-completion-help-json-contract",
            command: "completion",
            args: &["--help", "--json", "--wat"],
            expected_topic: "completion",
            expected_usage_fragment: "effigy completion <bash|zsh|fish> [--json]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-config-help-json-contract",
            command: "config",
            args: &["--help", "--json", "--wat"],
            expected_topic: "config",
            expected_usage_fragment: "effigy.toml Reference",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-init-help-json-contract",
            command: "init",
            args: &["--help", "--json", "--wat"],
            expected_topic: "init",
            expected_usage_fragment: "effigy init [--dry-run] [--force] [--json]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-watch-help-json-contract",
            command: "watch",
            args: &["--help", "--json", "--wat"],
            expected_topic: "watch",
            expected_usage_fragment: "effigy watch --owner <effigy|external>",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-migrate-help-json-contract",
            command: "migrate",
            args: &["--help", "--json", "--wat"],
            expected_topic: "migrate",
            expected_usage_fragment: "effigy migrate [--from <PATH>]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-doctor-help-json-contract",
            command: "doctor",
            args: &["--help", "--json", "--wat"],
            expected_topic: "doctor",
            expected_usage_fragment: "effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-scan-help-json-contract",
            command: "scan",
            args: &["--help", "--json", "--wat"],
            expected_topic: "scan",
            expected_usage_fragment:
                "effigy scan god-files [--threshold <N>] [--markdown] [--out <PATH>]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-tasks-help-json-contract",
            command: "tasks",
            args: &["--help", "--json", "--wat"],
            expected_topic: "tasks",
            expected_usage_fragment: "effigy tasks [--repo <PATH>] [--task <TASK_NAME>]",
        },
        BuiltinHelpJsonCase {
            workspace: "builtin-unlock-help-json-contract",
            command: "unlock",
            args: &["--help", "--json", "--wat"],
            expected_topic: "unlock",
            expected_usage_fragment: "effigy unlock [--all | <scope>...] [--json]",
        },
    ];

    assert_builtin_help_json_contract_case_table(&cases);
}
