use crate::runner::tests::prelude::{
    assert_output_contains_all, run_builtin_ok, temp_workspace, write_root_manifest,
};

#[test]
fn run_manifest_task_builtin_config_prints_reference() {
    let root = temp_workspace("builtin-config");
    write_root_manifest(&root, "");

    let out = run_builtin_ok(root, "config", &[]);
    assert_output_contains_all(
        &out,
        &[
            "effigy.toml Reference",
            "Use `effigy config --inspect` to inspect the effective composed manifest",
            "[manifest]",
            "cargo_env_match = \"prefix-aware\"",
            "[test.runners]",
            "[tasks]",
            "task = \"test vitest \\\"user service\\\"\"",
            "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\", depends_on = [\"tests\"] }]",
        ],
    );
}
