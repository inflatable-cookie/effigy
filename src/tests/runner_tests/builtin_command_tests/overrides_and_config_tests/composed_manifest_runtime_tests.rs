use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::assert_output_contains_all;
use crate::runner::tests::prelude::{temp_workspace, write_manifest, write_root_manifest};
use effigy_cli::{Command, DocsArgs, DocsCheckKind, DocsSubcommand};

#[test]
fn run_manifest_task_uses_docs_policy_loaded_from_composed_manifest() {
    let root = temp_workspace("composed-manifest-docs-policy-runtime");
    std::fs::create_dir_all(root.join("docs/vision")).expect("mkdir docs/vision");
    std::fs::write(
        root.join("docs/vision/README.md"),
        "# Vision\n\n## Vision Artifacts\n\n- [001-demo.md](./001-demo.md)\n",
    )
    .expect("write vision index");
    std::fs::write(
        root.join("docs/vision/001-demo.md"),
        "# Demo\n\n## Next Task\n\nBuild the next proof.\n",
    )
    .expect("write vision doc");

    write_root_manifest(
        &root,
        r#"
[manifest]
include = ["effigy.tasks.toml", "effigy.docs.toml"]
"#,
    );
    write_manifest(&root.join("effigy.tasks.toml"), "");
    write_manifest(
        &root.join("effigy.docs.toml"),
        r#"
[docs_policy.indexes.vision]
file = "docs/vision/README.md"
dir = "docs/vision"
section = "Vision Artifacts"
"#,
    );

    let out = run_command(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::Check {
            kind: DocsCheckKind::Index,
            paths: Vec::new(),
            file: None,
            section: None,
            min_blocks: None,
            required_text: Vec::new(),
            required_blocks: Vec::new(),
            required_headings: Vec::new(),
            forbidden_text: Vec::new(),
            policy_index: Some("vision".to_owned()),
            dir: None,
            index: None,
            policy_name: None,
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("run composed docs command");
    assert_output_contains_all(&out, &["docs index check passed"]);
}
