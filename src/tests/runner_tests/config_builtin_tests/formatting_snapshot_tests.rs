use crate::runner::tests::prelude::{
    assert_output_contains_all, run_config_ok, workspace_with_empty_manifest,
};

#[test]
fn run_manifest_task_builtin_config_has_blank_line_between_sections() {
    let root = workspace_with_empty_manifest("builtin-config-section-spacing");

    let out = run_config_ok(root, &[]);
    assert_output_contains_all(
        &out,
        &[
            "\n\nBundle\n",
            "\n\nGlobal\n",
            "\n\nBuilt-in Test\n",
            "\n\nTasks\n",
        ],
    );
}

#[test]
fn run_manifest_task_builtin_config_reference_mentions_bundle_discovery_and_named_schema_flag() {
    let root = workspace_with_empty_manifest("builtin-config-bundle-reference");

    let out = run_config_ok(root, &[]);
    assert_output_contains_all(
        &out,
        &[
            "Use `effigy bundle list` to discover shipped `[bundle]` presets",
            "Use `effigy config --schema --target bundle` for the generic `[bundle]` shape",
            "Use `[bundle].base = { type = \"path\", dir = \"...\" }` for repo-local bundle directories",
            "[bundle]",
            "base = \"decodelabs\"",
            "# base = { type = \"path\", dir = \"bundles/acme\" }",
            "Inspect one bundle: `effigy bundle inspect decodelabs`",
            "Render bundle config schema: `effigy config --schema --target bundle --bundle decodelabs`",
        ],
    );
}
