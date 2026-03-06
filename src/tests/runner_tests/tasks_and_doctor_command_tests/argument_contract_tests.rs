use super::prelude::{
    assert_builtin_argument_contract_command_case_table, BuiltinArgumentContractCase,
    BuiltinArgumentContractCommandCase,
};

#[test]
fn run_tasks_and_doctor_argument_contract_matrix() {
    let doctor_cases = [
        BuiltinArgumentContractCase {
            workspace: "doctor-contract-missing-repo-value",
            args: &["--repo"],
            expect_error: true,
            expected: &["task argument --repo requires a value"],
        },
        BuiltinArgumentContractCase {
            workspace: "doctor-contract-unknown-flags",
            args: &["--wat", "--huh"],
            expect_error: true,
            expected: &["unknown argument(s) for built-in `doctor`: --wat --huh"],
        },
        BuiltinArgumentContractCase {
            workspace: "doctor-contract-help-precedence",
            args: &["--help", "--wat"],
            expect_error: false,
            expected: &["doctor Help", "Usage"],
        },
    ];

    let tasks_cases = [
        BuiltinArgumentContractCase {
            workspace: "tasks-contract-missing-task-value",
            args: &["--task"],
            expect_error: true,
            expected: &["task argument --task requires a value"],
        },
        BuiltinArgumentContractCase {
            workspace: "tasks-contract-unknown-flags",
            args: &["--wat", "--huh"],
            expect_error: true,
            expected: &["unknown argument(s) for built-in `tasks`: --wat --huh"],
        },
        BuiltinArgumentContractCase {
            workspace: "tasks-contract-help-precedence",
            args: &["--help", "--wat"],
            expect_error: false,
            expected: &["tasks Help", "Usage"],
        },
    ];

    let command_cases = [
        BuiltinArgumentContractCommandCase {
            command: "doctor",
            cases: &doctor_cases,
        },
        BuiltinArgumentContractCommandCase {
            command: "tasks",
            cases: &tasks_cases,
        },
    ];

    assert_builtin_argument_contract_command_case_table(&command_cases);
}
