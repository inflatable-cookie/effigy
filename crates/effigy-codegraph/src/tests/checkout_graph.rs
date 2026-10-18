use super::*;
use crate::{node_with_options_in_scope, GraphScope, RefreshPolicy};

#[test]
fn one_checkout_graph_includes_all_catalog_members() {
    let temp = tempfile::tempdir().expect("private fixture");
    for (catalog, name) in [("a", "alpha"), ("b", "beta")] {
        let dir = temp.path().join(format!("apps/{catalog}/src"));
        fs::create_dir_all(&dir).expect("source dir");
        fs::write(dir.join("lib.rs"), format!("pub fn {name}() {{}}\n")).expect("source");
    }
    fs::write(
        temp.path().join("effigy.toml"),
        "[catalog]\nalias = 'root'\n[catalog.members]\na = 'apps/a'\nb = 'apps/b'\n",
    )
    .expect("manifest");
    run_index(temp.path()).expect("whole checkout index");
    let files = query_files(temp.path(), None).expect("files").files;
    for catalog in ["a", "b"] {
        assert!(files
            .iter()
            .any(|file| file.path == format!("apps/{catalog}/src/lib.rs")));
    }
    assert!(temp.path().join(".effigy/graph/graph.db").is_file());
    assert!(!temp.path().join(".effigy/graph/catalogs").exists());
    assert_eq!(
        GraphScope::workspace(temp.path()).expect("docs root"),
        GraphScope::repo_root(temp.path()).expect("code root")
    );
}

#[test]
fn node_combines_path_impact_and_both_call_directions() {
    let temp = tempfile::tempdir().expect("private fixture");
    fs::write(
        temp.path().join("a.rs"),
        "pub fn run_alpha() { target_beta(); }\n",
    )
    .expect("caller");
    fs::write(
        temp.path().join("b.rs"),
        "pub fn target_beta() {}\npub fn extra_beta() {}\n",
    )
    .expect("target");
    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    let alpha = symbols
        .iter()
        .find(|symbol| symbol.display_name == "run_alpha")
        .expect("alpha");
    let beta = symbols
        .iter()
        .find(|symbol| symbol.display_name == "target_beta")
        .expect("beta");
    let scope = GraphScope::repo_root(temp.path()).expect("root");
    let incoming = node_with_options_in_scope(
        &scope,
        beta.id.as_str(),
        true,
        false,
        Some(10),
        RefreshPolicy::query(),
    )
    .expect("incoming");
    assert_eq!(incoming.direction, "in");
    assert!(incoming
        .edges
        .iter()
        .any(|edge| edge.from_id == alpha.id
            && edge.unresolved_target.as_deref() == Some("target_beta")));
    assert!(incoming.symbols.iter().any(|symbol| symbol.id == alpha.id));
    let outgoing = node_with_options_in_scope(
        &scope,
        alpha.id.as_str(),
        false,
        true,
        Some(10),
        RefreshPolicy::query(),
    )
    .expect("outgoing");
    assert_eq!(outgoing.direction, "out");
    assert!(outgoing.symbols.iter().any(|symbol| symbol.id == beta.id));
    assert!(outgoing.edges.iter().all(|edge| edge.from_id == alpha.id));
    let path = node_with_options_in_scope(
        &scope,
        "b.rs",
        false,
        false,
        Some(10),
        RefreshPolicy::query(),
    )
    .expect("path");
    assert_eq!(path.file.as_ref().expect("target file").path, "b.rs");
    assert!(path
        .symbols
        .iter()
        .any(|symbol| symbol.display_name == "extra_beta"));
    assert!(path.edges.iter().any(|edge| edge.from_id == alpha.id));
    let bounded = node_with_options_in_scope(
        &scope,
        "b.rs",
        false,
        false,
        Some(1),
        RefreshPolicy::query(),
    )
    .expect("bounded");
    assert!(
        bounded.edges.len() <= 1
            && bounded.symbols.len() <= 1
            && bounded.files.len() <= 1
            && bounded.references.len() <= 1
            && bounded.diagnostics.len() <= 1
    );
}
