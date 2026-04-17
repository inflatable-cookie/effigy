use crate::runner::tests::prelude::{
    assert_managed_output_derived_case_table, lock_test, managed_tui_env,
    write_catalog_manifest_with_alias, write_froyo_validate_catalog, ManagedInvocation,
    ManagedOutputDerivedCase, Path,
};

fn setup_relative_task_refs(root: &Path) {
    write_catalog_manifest_with_alias(
        root,
        "catalog_b",
        "catalog_b",
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "validate-stack", task = "../froyo/validate" }]
"#,
    );
    write_froyo_validate_catalog(root);
}

fn expected_froyo_catalog_path(root: &Path) -> Vec<String> {
    vec![root.join("froyo").display().to_string()]
}

#[test]
fn run_manifest_task_managed_tui_relative_task_ref_contract_table() {
    let _guard = lock_test();
    let _env = managed_tui_env();
    let cases = [ManagedOutputDerivedCase {
        workspace: "managed-relative-task-ref",
        invocation: ManagedInvocation::TaskWithRepo("catalog_b/dev"),
        args: &[],
        expected: &["validate-stack", "froyo-validate"],
        expected_absent: &[],
        expected_derived: expected_froyo_catalog_path,
        setup: setup_relative_task_refs,
    }];

    assert_managed_output_derived_case_table(&cases);
}
