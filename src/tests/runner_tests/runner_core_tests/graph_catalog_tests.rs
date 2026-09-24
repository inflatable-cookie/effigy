//! Catalog-scoped graph command surface (task g10.006).

use crate::runner::entrypoints::{run_command, run_command_with_context};
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_root_manifest,
};
use effigy_cli::{Command, GraphArgs, GraphSubcommand};
use effigy_context::{CapturedEnv, EffigyRuntimeContext};
use std::fs;
use std::path::{Path, PathBuf};

/// Root source plus two segmented catalogs, one of them independent.
fn setup_catalog_fixture(name: &str) -> PathBuf {
    let root = temp_workspace(name);
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("apps/bovine/src")).expect("mkdir bovine");
    fs::create_dir_all(root.join("apps/farmyard/src")).expect("mkdir farmyard");
    write_root_manifest(
        &root,
        r#"
[catalog.members]
bovine = "apps/bovine"
farmyard = "apps/farmyard"

[tasks.build]
run = "cargo test"
"#,
    );
    fs::write(root.join("src/lib.rs"), "pub fn root_symbol() {}\n").expect("root source");
    fs::write(
        root.join("apps/bovine/effigy.toml"),
        "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true\nindependent = true\n\n[tasks.build]\nrun = \"echo bovine\"\n",
    )
    .expect("bovine manifest");
    fs::write(
        root.join("apps/bovine/src/lib.rs"),
        "pub fn bovine_symbol() {}\n",
    )
    .expect("bovine source");
    fs::write(
        root.join("apps/farmyard/effigy.toml"),
        "[catalog]\nalias = \"farmyard\"\n\n[catalog.graph]\nsegmented = true\n\n[tasks.build]\nrun = \"echo farmyard\"\n",
    )
    .expect("farmyard manifest");
    fs::write(
        root.join("apps/farmyard/src/lib.rs"),
        "pub fn farmyard_symbol() {}\n",
    )
    .expect("farmyard source");
    root
}

fn graph_args(subcommand: GraphSubcommand, root: &Path) -> GraphArgs {
    GraphArgs {
        subcommand,
        repo_override: Some(root.to_path_buf()),
        output_json: true,
        catalog: None,
        all_catalogs: false,
    }
}

#[test]
fn graph_index_catalog_json_names_the_scope_and_isolates_storage() {
    let root = setup_catalog_fixture("graph-catalog-index");

    let mut args = graph_args(GraphSubcommand::Index, &root);
    args.catalog = Some("bovine".to_owned());
    let rendered = run_command(Command::Graph(args)).expect("graph index should succeed");
    let parsed = parse_json_output_with_schema_version(&rendered, "effigy.graph.index.v1", 1);

    assert_eq!(parsed["catalog"]["alias"], "bovine");
    assert_eq!(parsed["catalog"]["root"], "apps/bovine");
    assert_eq!(parsed["catalog"]["segmented"], true);
    assert_eq!(parsed["catalog"]["independent"], true);
    assert_eq!(parsed["catalog"]["selection"], "explicit");
    assert!(parsed["payload"]["indexed_files"].as_u64().unwrap_or(0) >= 1);

    let expected_db = root
        .canonicalize()
        .expect("canonical")
        .join(".effigy/graph/catalogs/bovine/graph.db");
    assert!(expected_db.is_file(), "independent database missing");
    assert!(
        !root.join(".effigy/graph/graph.db").exists(),
        "independent catalog wrote the shared database"
    );
    assert!(
        !root
            .join(".effigy/graph/catalogs/farmyard/graph.db")
            .exists(),
        "independent catalog touched a sibling database"
    );

    // Root query must not report sibling files.
    let root_files = run_command(Command::Graph(graph_args(
        GraphSubcommand::Files { limit: None },
        &root,
    )))
    .expect("root files");
    let parsed = parse_json_output_with_schema_version(&root_files, "effigy.graph.files.v1", 1);
    assert_eq!(parsed["catalog"]["alias"], "root");
    let paths = parsed["payload"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"src/lib.rs"), "{paths:?}");
    assert!(
        !paths.iter().any(|path| path.starts_with("apps/")),
        "root scope indexed a segmented catalog: {paths:?}"
    );

    // The catalog query reports only its own files.
    let mut args = graph_args(GraphSubcommand::Files { limit: None }, &root);
    args.catalog = Some("bovine".to_owned());
    let bovine_files = run_command(Command::Graph(args)).expect("bovine files");
    let parsed = parse_json_output_with_schema_version(&bovine_files, "effigy.graph.files.v1", 1);
    let paths = parsed["payload"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|file| file["path"].as_str())
        .collect::<Vec<_>>();
    assert!(
        paths.iter().all(|path| path.starts_with("apps/bovine")),
        "catalog query escaped its scope: {paths:?}"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn graph_cwd_selection_uses_captured_invocation_context() {
    let root = setup_catalog_fixture("graph-captured-cwd");
    let context = EffigyRuntimeContext::builder()
        .cwd_override(Some(root.join("apps/bovine")))
        .captured_env(CapturedEnv::default())
        .capture()
        .expect("capture member invocation");
    let rendered = run_command_with_context(
        Command::Graph(graph_args(GraphSubcommand::Index, &root)),
        &context,
    )
    .expect("index selected member");
    let parsed = parse_json_output_with_schema_version(&rendered, "effigy.graph.index.v1", 1);

    assert_eq!(parsed["catalog"]["alias"], "bovine");
    assert_eq!(parsed["catalog"]["selection"], "cwd");
    assert!(root
        .join(".effigy/graph/catalogs/bovine/graph.db")
        .is_file());
    assert!(!root.join(".effigy/graph/graph.db").exists());

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn graph_unknown_or_folded_catalog_fails_before_creating_state() {
    let root = setup_catalog_fixture("graph-catalog-unknown");

    let mut args = graph_args(GraphSubcommand::Index, &root);
    args.catalog = Some("missing".to_owned());
    let error = run_command(Command::Graph(args)).expect_err("unknown catalog must fail");
    assert!(error.to_string().contains("unknown catalog"), "{error}");
    assert!(error.to_string().contains("bovine"), "{error}");
    assert!(error.to_string().contains("farmyard"), "{error}");
    assert!(
        !root.join(".effigy").exists(),
        "failed selection created graph state"
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn graph_all_catalogs_reports_one_outcome_per_scope() {
    let root = setup_catalog_fixture("graph-catalog-fanout");

    let mut args = graph_args(GraphSubcommand::Index, &root);
    args.all_catalogs = true;
    let rendered = run_command(Command::Graph(args)).expect("fan-out index should succeed");
    let parsed = parse_json_output_with_schema_version(&rendered, "effigy.graph.fanout.v1", 1);

    let catalogs = parsed["payload"]["catalogs"]
        .as_array()
        .expect("catalog outcomes");
    assert_eq!(catalogs.len(), 3, "{rendered}");
    let aliases = catalogs
        .iter()
        .map(|outcome| outcome["catalog"]["alias"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(aliases, vec!["root", "bovine", "farmyard"]);
    assert!(
        catalogs
            .iter()
            .all(|outcome| outcome["ok"].as_bool() == Some(true)),
        "{rendered}"
    );
    assert!(
        root.join(".effigy/graph/graph.db").is_file(),
        "shared scope database missing"
    );
    assert!(
        root.join(".effigy/graph/catalogs/bovine/graph.db")
            .is_file(),
        "independent scope database missing"
    );

    let _ = fs::remove_dir_all(&root);
}
