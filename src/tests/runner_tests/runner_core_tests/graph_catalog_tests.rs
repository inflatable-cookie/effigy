use crate::runner::entrypoints::run_command;
use effigy_cli::{Command, GraphArgs, GraphSubcommand};

#[test]
fn graph_checkout_index_includes_all_member_files_without_catalog_payload() {
    let fixture = tempfile::tempdir().expect("private fixture");
    let root = fixture.path();
    std::fs::write(
        root.join("effigy.toml"),
        "[catalog]\nalias = 'root'\n[catalog.members]\na = 'apps/a'\nb = 'apps/b'\n",
    )
    .expect("root manifest");
    for alias in ["a", "b"] {
        let member = root.join(format!("apps/{alias}"));
        std::fs::create_dir_all(member.join("src")).expect("member source");
        std::fs::write(
            member.join("effigy.toml"),
            format!("[catalog]\nalias = '{alias}'\n"),
        )
        .expect("member manifest");
        std::fs::write(
            member.join("src/lib.rs"),
            format!("pub fn member_{alias}() {{}}\n"),
        )
        .expect("source");
    }
    let output = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.to_path_buf()),
        output_json: true,
        stale_index: false,
    }))
    .expect("index");
    let output: serde_json::Value = serde_json::from_str(&output).expect("json");
    assert!(output.get("catalog").is_none());
    let store = effigy_codegraph::GraphStore::open(root).expect("store");
    for alias in ["a", "b"] {
        assert!(store
            .list_files()
            .expect("files")
            .iter()
            .any(|file| file.path == format!("apps/{alias}/src/lib.rs")));
    }
    assert!(!root.join(".effigy/graph/catalogs").exists());
}
