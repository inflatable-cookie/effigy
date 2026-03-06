use super::prelude::{
    assert_builtin_error_case_table_with_case_setup, write_root_manifest,
    BuiltinInvocationSetupCase, Path,
};

fn setup_root_catalog_manifest(root: &Path) {
    write_root_manifest(root, "[tasks.root]\nrun = \"printf root\"\n");
}

#[test]
fn run_manifest_task_builtin_catalogs_argument_validation_contract_table() {
    let cases = [
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-pretty-requires-json",
            args: &["--pretty", "false"],
            expected: &["`--pretty` is only supported together with `--json`"],
            setup: setup_root_catalog_manifest,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-invalid-pretty",
            args: &["--json", "--pretty", "nope"],
            expected: &["value `nope` is invalid"],
            setup: setup_root_catalog_manifest,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-missing-resolve-value",
            args: &["--resolve"],
            expected: &["catalogs argument --resolve requires a value"],
            setup: setup_root_catalog_manifest,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-missing-pretty-value",
            args: &["--json", "--pretty"],
            expected: &["catalogs argument --pretty requires a value (`true` or `false`)"],
            setup: setup_root_catalog_manifest,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-missing-task-value",
            args: &["--task"],
            expected: &["task argument --task requires a value"],
            setup: setup_root_catalog_manifest,
        },
        BuiltinInvocationSetupCase {
            workspace: "builtin-catalogs-unknown-args",
            args: &["--wat", "--huh"],
            expected: &["unknown argument(s) for built-in `catalogs`: --wat --huh"],
            setup: setup_root_catalog_manifest,
        },
    ];

    assert_builtin_error_case_table_with_case_setup("catalogs", &cases);
}
