use crate::runner::tests::prelude::{
    assert_builtin_error_contract_case_table, builtin_contract_error_case,
    builtin_shared_unknown_argument_cases,
};

#[test]
fn run_manifest_task_builtin_argument_contract_matrix_is_stable() {
    let mut cases = vec![
        // `doctor --wat` is pinned by tasks_and_doctor_command_tests
        // (`doctor --wat --huh` renders the same message superset) and
        // `tasks --task` by that family's tasks matrix; the completion
        // candidates `--prefix` requires-value row lives in help_tests'
        // completion argument-validation table.
        builtin_contract_error_case(
            "builtin-arg-contract-doctor-env-schema-unsupported",
            "doctor",
            &["--env-schema", "config/test.env.schema"],
            &["`--env-schema` is not supported for built-in `doctor`"],
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
            "builtin-arg-contract-init-unknown",
            "init",
            &["--wat"],
            &["`init` received unknown flag(s): --wat"],
        ),
    ];
    cases.extend(builtin_shared_unknown_argument_cases(
        "builtin-arg-contract-completion-candidates-unknown",
        "builtin-arg-contract-scan-unknown-flag",
    ));

    assert_builtin_error_contract_case_table(&cases);
}
