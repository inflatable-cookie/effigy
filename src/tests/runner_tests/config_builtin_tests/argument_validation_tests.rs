use super::prelude::*;

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_flag_combinations() {
    let cases = [
        ConfigErrorCase {
            workspace: "builtin-config-target-requires-schema",
            args: &["--target", "test"],
            expected: &["`--target` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-runner-requires-schema",
            args: &["--runner", "vitest"],
            expected: &["`--runner` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-runner-requires-test-target",
            args: &["--schema", "--target", "tasks", "--runner", "vitest"],
            expected: &["`--runner` requires `--target test`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-invalid-runner",
            args: &["--schema", "--target", "test", "--runner", "jest"],
            expected: &["invalid `--runner` value `jest`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-target-requires-value",
            args: &["--schema", "--target"],
            expected: &["`--target` requires a value"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-invalid-target",
            args: &["--schema", "--target", "python"],
            expected: &["invalid `--target` value `python`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-minimal-requires-schema",
            args: &["--minimal"],
            expected: &["`--minimal` requires `--schema`"],
        },
        ConfigErrorCase {
            workspace: "builtin-config-unknown-args",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `config`: --wat"],
        },
    ];

    for case in cases {
        let root = workspace_with_empty_manifest(case.workspace);
        let err = run_config_err(root, case.args);
        assert_task_invocation_error_contains(err, case.expected);
    }
}
