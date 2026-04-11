use super::prelude::{
    assert_builtin_error_case_table_with_setup, write_root_manifest, BuiltinInvocationCase,
};

#[test]
fn run_manifest_task_builtin_config_rejects_invalid_flag_combinations() {
    let cases = [
        BuiltinInvocationCase {
            workspace: "builtin-config-target-requires-schema",
            args: &["--target", "test"],
            expected: &["`--target` requires `--schema`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-runner-requires-schema",
            args: &["--runner", "vitest"],
            expected: &["`--runner` requires `--schema`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-runner-requires-test-target",
            args: &["--schema", "--target", "tasks", "--runner", "vitest"],
            expected: &["`--runner` requires `--target test`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-invalid-runner",
            args: &["--schema", "--target", "test", "--runner", "jest"],
            expected: &["invalid `--runner` value `jest`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-target-requires-value",
            args: &["--schema", "--target"],
            expected: &["`--target` requires a value"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-invalid-target",
            args: &["--schema", "--target", "python"],
            expected: &["invalid `--target` value `python`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-minimal-requires-schema",
            args: &["--minimal"],
            expected: &["`--minimal` requires `--schema`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-inspect-conflicts-with-schema",
            args: &["--inspect", "--schema"],
            expected: &["`--inspect` cannot be combined with `--schema`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-unknown-args",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `config`: --wat"],
        },
    ];
    assert_builtin_error_case_table_with_setup("config", &cases, |root| {
        write_root_manifest(root, "");
    });
}
