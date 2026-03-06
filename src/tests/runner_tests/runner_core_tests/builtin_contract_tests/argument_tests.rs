use super::prelude::{assert_builtin_error_contract_case_table, BuiltinContractErrorCase};

#[test]
fn run_manifest_task_builtin_argument_contract_matrix_is_stable() {
    let cases = [
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-doctor-unknown",
            command: "doctor",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `doctor`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-tasks-missing-task",
            command: "tasks",
            args: &["--task"],
            expected: &["task argument --task requires a value"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-tasks-missing-resolve",
            command: "tasks",
            args: &["--resolve"],
            expected: &["tasks argument --resolve requires a value"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-config-missing-target",
            command: "config",
            args: &["--schema", "--target"],
            expected: &["`--target` requires a value for built-in `config`"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-watch-missing-owner",
            command: "watch",
            args: &["--owner"],
            expected: &["`--owner` requires a value (`effigy` or `external`)"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-watch-unknown",
            command: "watch",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `watch`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-completion-candidates-missing-prefix",
            command: "completion",
            args: &["candidates", "--prefix"],
            expected: &["completion candidates argument --prefix requires a value"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-init-unknown",
            command: "init",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `init`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-migrate-missing-from",
            command: "migrate",
            args: &["--from"],
            expected: &["`--from` requires a file path"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-migrate-missing-script",
            command: "migrate",
            args: &["--script"],
            expected: &["`--script` requires a script name"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-migrate-unknown",
            command: "migrate",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `migrate`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-unlock-unknown",
            command: "unlock",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `unlock`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-cache-missing-subcommand",
            command: "cache",
            args: &[],
            expected: &["`cache` requires a subcommand: `inspect` or `invalidate`"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-cache-unknown-flag",
            command: "cache",
            args: &["inspect", "--wat"],
            expected: &["unknown argument(s) for built-in `cache`: --wat"],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-scan-missing-subcommand",
            command: "scan",
            args: &[],
            expected: &[
                "scan requires a subcommand (currently supported: `god-files`, `duplicate-blocks`, `comment-ratio`, `generated-assets`, `attention-markers`)",
            ],
        },
        BuiltinContractErrorCase {
            workspace: "builtin-arg-contract-scan-unknown-flag",
            command: "scan",
            args: &["god-files", "--wat"],
            expected: &["unknown argument(s) for built-in `scan`: --wat"],
        },
    ];

    assert_builtin_error_contract_case_table(&cases);
}
