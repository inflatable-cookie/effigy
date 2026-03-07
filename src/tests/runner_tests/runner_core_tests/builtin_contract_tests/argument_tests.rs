use super::prelude::{
    assert_builtin_error_contract_case_table, builtin_contract_error_case,
    builtin_shared_unknown_argument_cases,
};

#[test]
fn run_manifest_task_builtin_argument_contract_matrix_is_stable() {
    let mut cases = vec![
        builtin_contract_error_case(
            "builtin-arg-contract-doctor-unknown",
            "doctor",
            &["--wat"],
            &["unknown argument(s) for built-in `doctor`: --wat"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-tasks-missing-task",
            "tasks",
            &["--task"],
            &["task argument --task requires a value"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-tasks-missing-resolve",
            "tasks",
            &["--resolve"],
            &["tasks argument --resolve requires a value"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-config-missing-target",
            "config",
            &["--schema", "--target"],
            &["`--target` requires a value for built-in `config`"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-watch-missing-owner",
            "watch",
            &["--owner"],
            &["`--owner` requires a value (`effigy` or `external`)"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-watch-unknown",
            "watch",
            &["--wat"],
            &["unknown argument(s) for built-in `watch`: --wat"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-completion-candidates-missing-prefix",
            "completion",
            &["candidates", "--prefix"],
            &["completion candidates argument --prefix requires a value"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-init-unknown",
            "init",
            &["--wat"],
            &["unknown argument(s) for built-in `init`: --wat"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-migrate-missing-from",
            "migrate",
            &["--from"],
            &["`--from` requires a file path"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-migrate-missing-script",
            "migrate",
            &["--script"],
            &["`--script` requires a script name"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-migrate-unknown",
            "migrate",
            &["--wat"],
            &["unknown argument(s) for built-in `migrate`: --wat"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-unlock-unknown",
            "unlock",
            &["--wat"],
            &["unknown argument(s) for built-in `unlock`: --wat"],
        ),
        builtin_contract_error_case(
            "builtin-arg-contract-cache-missing-subcommand",
            "cache",
            &[],
            &["`cache` requires a subcommand: `inspect` or `invalidate`"],
        ),
    ];
    cases.extend(builtin_shared_unknown_argument_cases(
        "builtin-arg-contract-cache-unknown-flag",
        "builtin-arg-contract-completion-candidates-unknown",
        "builtin-arg-contract-watch-unknown",
        "builtin-arg-contract-scan-unknown-flag",
        "builtin-arg-contract-unlock-unknown",
    ));

    assert_builtin_error_contract_case_table(&cases);
}
