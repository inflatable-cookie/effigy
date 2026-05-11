use crate::runner::tests::prelude::{
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
            workspace: "builtin-config-bundle-requires-schema",
            args: &["--bundle", "decodelabs-library"],
            expected: &["`--bundle` requires `--schema`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-bundle-requires-bundle-target",
            args: &["--schema", "--target", "tasks", "--bundle", "decodelabs-library"],
            expected: &["`--bundle` requires `--target bundle`"],
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
            workspace: "builtin-config-bundle-requires-value",
            args: &["--schema", "--target", "bundle", "--bundle"],
            expected: &["`--bundle` requires a value"],
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
            workspace: "builtin-config-path-requires-inspect",
            args: &["--path", "tasks.dev"],
            expected: &["`--path` requires `--inspect`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-path-requires-value",
            args: &["--inspect", "--path"],
            expected: &["`--path` requires a value"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-unknown-args",
            args: &["--wat"],
            expected: &["unknown argument(s) for built-in `config`: --wat"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-user-inspect-conflicts-with-schema",
            args: &["--user-inspect", "--schema"],
            expected: &[
                "user-global config flags cannot be combined with `--inspect` or `--schema`",
            ],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-user-path-conflict",
            args: &["--user-inspect", "--path", "tasks.dev"],
            expected: &["`--path` cannot be combined with user-global config flags"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-user-inspect-conflicts-with-update",
            args: &["--user-inspect", "--set-container-backend", "containerd"],
            expected: &[
                "`--user-inspect` cannot be combined with other user-global config operations",
            ],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-container-backend-conflict",
            args: &[
                "--set-container-backend",
                "containerd",
                "--unset-container-backend",
            ],
            expected: &[
                "`--set-container-backend` cannot be combined with `--unset-container-backend`",
            ],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-container-profile-conflict",
            args: &[
                "--set-container-profile",
                "effigy",
                "--unset-container-profile",
            ],
            expected: &[
                "`--set-container-profile` cannot be combined with `--unset-container-profile`",
            ],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-container-backend-requires-value",
            args: &["--set-container-backend"],
            expected: &["`--set-container-backend` requires a value"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-container-backend-invalid",
            args: &["--set-container-backend", "podman"],
            expected: &["invalid `--set-container-backend` value `podman`"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-container-profile-requires-value",
            args: &["--set-container-profile"],
            expected: &["`--set-container-profile` requires a value"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-get-requires-key",
            args: &["get"],
            expected: &["`config get` requires a subcommand"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-set-requires-key",
            args: &["set"],
            expected: &["`config set` requires a subcommand"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-unset-requires-key",
            args: &["unset"],
            expected: &["`config unset` requires a subcommand"],
        },
        BuiltinInvocationCase {
            workspace: "builtin-config-get-unknown-key",
            args: &["get", "containers.unknown"],
            expected: &["unknown config get key `containers.unknown`"],
        },
    ];
    assert_builtin_error_case_table_with_setup("config", &cases, |root| {
        write_root_manifest(root, "");
    });
}
