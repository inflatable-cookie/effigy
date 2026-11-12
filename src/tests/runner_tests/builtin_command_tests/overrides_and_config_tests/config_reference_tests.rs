use crate::runner::tests::prelude::{run_builtin_ok, temp_workspace, write_root_manifest};

#[test]
fn run_manifest_task_builtin_config_prints_reference() {
    let root = temp_workspace("builtin-config");
    write_root_manifest(&root, "");

    let out = run_builtin_ok(root, "config", &[]);
    for expected in [
        "effigy.toml Reference",
        "• info: Use `effigy config --inspect` to inspect the effective composed manifest for the current repo.",
        "• info: Use `effigy config path|get|set|unset` for user-global machine settings, or `effigy config --user-inspect` for the full rendered user config without editing `~/.effigy/config.toml` by hand.",
        "• info: Use `effigy bundle inspect` to inspect the active repo bundle source and `effigy bundle sync` to update checkout pins for path, git or OCI bundle sources.",
        "• info: Use `[bundle].base = { type = \"path\", dir = \"...\" }` for repo-local bundle directories that carry `bundle.toml` metadata plus an `effigy.toml` defaults template.",
        "# Inspect the active repo bundle source: `effigy bundle inspect`",
        "# Refresh remote git or OCI sources: `effigy bundle sync`",
        "# Render the generic bundle config schema: `effigy config --schema --target bundle`",
        "# Or use a git-hosted bundle:",
        "base = { type = \"path\", dir = \"bundles/acme\" }",
        "[bundle]",
        "[manifest]",
        "# Optional TCP alias and container-host cookie apex.",
        "# base_domain = \"project.test\"",
        "[docs_policy.graph]",
        "roots = [\"README.md\", \"docs\"]",
        "[docs_policy.graph.fields.state]",
        "[docs_policy.graph.kinds.reference]",
        "cargo_env_match = \"prefix-aware\"",
        "[test.runners]",
        "[tasks]",
        "run = [{ id = \"tests\", task = \"test vitest \\\"user service\\\"\" }, { id = \"report\", run = \"printf validate-ok\" }]",
    ] {
        assert!(
            out.lines().any(|line| line == expected),
            "reference output must include {expected:?}: {out}"
        );
    }
    for heading in ["Bundle", "Global", "Built-in Test", "Tasks"] {
        assert!(
            out.contains(&format!("\n\n{heading}\n")),
            "reference sections keep their blank-line boundary before {heading}: {out}"
        );
    }
}
