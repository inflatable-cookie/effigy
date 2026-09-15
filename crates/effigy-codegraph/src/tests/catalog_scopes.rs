//! Catalog-scoped graph proofs (task g10.006).
//!
//! These fixtures exercise the adversarial oracle from the task: one catalog
//! must never pay a sibling's cost, root scans must prune segmented trees
//! structurally, shared storage must stay scoped, and independent storage must
//! be physically isolated.

use super::*;

use crate::json::{GraphCatalogPayload, GraphCommandPayload};
use crate::scope::{build_scopes, select_scopes, GraphScope, GraphScopePlan, GraphScopeRequest};
use crate::walk;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn write_manifest(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("manifest parent")).expect("catalog dir");
    fs::write(path, body).expect("manifest");
}

fn write_source(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("source parent")).expect("source dir");
    fs::write(path, body).expect("source");
}

/// Build a `LoadedCatalog` the way effective membership would, without
/// pulling catalog routing into this crate.
fn loaded_catalog(
    alias: &str,
    catalog_root: &Path,
    manifest_body: &str,
    depth: usize,
) -> effigy_manifest::LoadedCatalog {
    let manifest_path = catalog_root.join("effigy.toml");
    write_manifest(&manifest_path, manifest_body);
    effigy_manifest::LoadedCatalog {
        alias: alias.to_owned(),
        catalog_root: catalog_root.to_path_buf(),
        manifest_path,
        bundle_root: None,
        manifest: toml::from_str(manifest_body).expect("parse manifest"),
        defer_run: None,
        deferred_builtins: BTreeSet::new(),
        depth,
        draft_sources: Default::default(),
    }
}

struct CatalogFixture {
    root: PathBuf,
}

/// Two segmented catalogs, one folded member, and loose root source.
fn two_catalog_fixture(prefix: &str, bovine_independent: bool) -> CatalogFixture {
    let root = std::env::temp_dir().join(format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("root");
    write_source(&root.join("src/lib.rs"), "pub fn root_symbol() {}\n");
    write_source(
        &root.join("apps/bovine/src/lib.rs"),
        "pub fn bovine_symbol() {}\n",
    );
    write_source(
        &root.join("apps/farmyard/src/lib.rs"),
        "pub fn farmyard_symbol() {}\n",
    );
    write_source(
        &root.join("apps/folded/src/lib.rs"),
        "pub fn folded_symbol() {}\n",
    );

    let bovine_body = if bovine_independent {
        "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true\nindependent = true\n"
    } else {
        "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true\n"
    };
    loaded_catalog("bovine", &root.join("apps/bovine"), bovine_body, 1);
    loaded_catalog(
        "farmyard",
        &root.join("apps/farmyard"),
        "[catalog]\nalias = \"farmyard\"\n\n[catalog.graph]\nsegmented = true\n",
        1,
    );
    loaded_catalog(
        "folded",
        &root.join("apps/folded"),
        "[catalog]\nalias = \"folded\"\n",
        1,
    );
    loaded_catalog("root", &root, "[catalog]\nalias = \"root\"\n", 0);

    CatalogFixture { root }
}

fn catalogs_for(fixture: &CatalogFixture) -> Vec<effigy_manifest::LoadedCatalog> {
    // Rebuild the same effective membership the fixture wrote.
    let mut catalogs = vec![
        loaded_catalog(
            "bovine",
            &fixture.root.join("apps/bovine"),
            &fs::read_to_string(fixture.root.join("apps/bovine/effigy.toml")).expect("bovine"),
            1,
        ),
        loaded_catalog(
            "farmyard",
            &fixture.root.join("apps/farmyard"),
            &fs::read_to_string(fixture.root.join("apps/farmyard/effigy.toml")).expect("farmyard"),
            1,
        ),
        loaded_catalog(
            "folded",
            &fixture.root.join("apps/folded"),
            &fs::read_to_string(fixture.root.join("apps/folded/effigy.toml")).expect("folded"),
            1,
        ),
        loaded_catalog(
            "root",
            &fixture.root,
            &fs::read_to_string(fixture.root.join("effigy.toml")).expect("root"),
            0,
        ),
    ];
    catalogs.sort_by(|left, right| left.alias.cmp(&right.alias));
    catalogs
}

fn scope_by_alias(scopes: &[GraphScope], alias: &str) -> GraphScope {
    scopes
        .iter()
        .find(|scope| scope.alias() == alias)
        .cloned()
        .unwrap_or_else(|| panic!("missing scope {alias}"))
}

fn scan_paths(scope: &GraphScope) -> Vec<String> {
    walk::scan_repo_files_in_scope(scope)
        .expect("scoped walk")
        .into_iter()
        .map(|entry| entry.relative_path)
        .collect()
}

#[test]
fn root_scope_prunes_every_segmented_catalog_before_descent() {
    let fixture = two_catalog_fixture("graph-root-prune", false);
    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let root = scope_by_alias(&scopes, "root");
    let paths = scan_paths(&root);

    assert!(paths.contains(&"src/lib.rs".to_owned()), "{paths:?}");
    assert!(
        paths.contains(&"apps/folded/src/lib.rs".to_owned()),
        "folded member source stays in the parent corpus: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.starts_with("apps/bovine")),
        "root scan entered a segmented catalog: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.starts_with("apps/farmyard")),
        "root scan entered a segmented catalog: {paths:?}"
    );
    let _ = fs::remove_dir_all(&fixture.root);
}

#[test]
fn selecting_one_catalog_never_reads_a_sibling_tree() {
    let fixture = two_catalog_fixture("graph-no-sibling", false);
    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");

    // Instrumentation: an unreadable sibling directory fails the walk if the
    // walker ever descends into it. Pruning must keep the walk green.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let blocked = fixture.root.join("apps/farmyard/blocked");
        fs::create_dir_all(&blocked).expect("blocked dir");
        fs::write(blocked.join("huge.rs"), "pub fn hidden() {}\n").expect("blocked file");
        fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000)).expect("chmod");
    }

    let paths = scan_paths(&bovine);
    assert!(
        paths.iter().all(|path| path.starts_with("apps/bovine")),
        "catalog scan escaped its scope: {paths:?}"
    );
    assert!(
        !paths.iter().any(|path| path.contains("farmyard")),
        "catalog scan touched the sibling: {paths:?}"
    );

    run_index_in_scope(&bovine).expect("index bovine");
    let store = GraphStore::open_for_scope(&bovine).expect("open shared store");
    let files = store.list_files().expect("files");
    assert!(
        files
            .iter()
            .all(|file| file.path.starts_with("apps/bovine")),
        "shared database holds sibling rows: {:?}",
        files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>()
    );

    let search =
        crate::search_in_scope(&bovine, "farmyard_symbol", Some(10)).expect("scoped search");
    assert!(
        search.matches.is_empty(),
        "scoped query returned sibling results: {:?}",
        search
            .matches
            .iter()
            .map(|entry| entry.record_id.as_str())
            .collect::<Vec<_>>()
    );

    let farmyard = scope_by_alias(&scopes, "farmyard");
    let farmyard_store = GraphStore::open_for_scope(&farmyard).expect("shared store");
    assert_eq!(
        farmyard_store
            .list_files()
            .expect("files")
            .iter()
            .filter(|file| file.path.starts_with("apps/farmyard"))
            .count(),
        0,
        "selecting one catalog indexed a sibling"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(
            fixture.root.join("apps/farmyard/blocked"),
            fs::Permissions::from_mode(0o755),
        );
    }
    let _ = fs::remove_dir_all(&fixture.root);
}

#[test]
fn shared_store_refresh_keeps_sibling_records() {
    let fixture = two_catalog_fixture("graph-shared-isolation", false);
    let catalogs = catalogs_for(&fixture);
    let scopes = build_scopes(&fixture.root, &catalogs).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");
    let farmyard = scope_by_alias(&scopes, "farmyard");

    run_index_in_scope(&bovine).expect("index bovine");
    run_index_in_scope(&farmyard).expect("index farmyard");

    // Reindex bovine after a change: farmyard rows must survive untouched.
    write_source(
        &fixture.root.join("apps/bovine/src/lib.rs"),
        "pub fn bovine_symbol() {}\npub fn bovine_added() {}\n",
    );
    run_index_in_scope(&bovine).expect("reindex bovine");

    let store = GraphStore::open_for_scope(&bovine).expect("shared store");
    let files = store.list_files().expect("files");
    assert!(
        files
            .iter()
            .any(|file| file.path == "apps/farmyard/src/lib.rs"),
        "reindexing one catalog deleted a sibling's records"
    );
    let bovine_counts = store.counts_in_scope(&bovine).expect("bovine counts");
    let farmyard_counts = store.counts_in_scope(&farmyard).expect("farmyard counts");
    assert!(bovine_counts.files >= 1);
    assert!(farmyard_counts.files >= 1);
    assert!(
        store
            .list_files_in_scope(&bovine)
            .expect("bovine files")
            .iter()
            .all(|file| file.path.starts_with("apps/bovine")),
        "scope counts leaked sibling rows"
    );

    let farmyard_search =
        crate::search_in_scope(&farmyard, "farmyard_symbol", Some(10)).expect("scoped search");
    assert!(
        farmyard_search
            .matches
            .iter()
            .any(|entry| entry.path.as_deref() == Some("apps/farmyard/src/lib.rs")),
        "sibling records were lost: {:?}",
        farmyard_search
            .matches
            .iter()
            .map(|entry| entry.record_id.as_str())
            .collect::<Vec<_>>()
    );
    let _ = fs::remove_dir_all(&fixture.root);
}

#[test]
fn independent_catalog_uses_only_its_own_database_and_lock() {
    let fixture = two_catalog_fixture("graph-independent", true);
    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");
    let paths = bovine.paths();

    let canonical_root = fixture.root.canonicalize().expect("canonical");
    assert_eq!(
        paths.db_path,
        canonical_root.join(".effigy/graph/catalogs/bovine/graph.db")
    );
    assert_eq!(
        paths.refresh_lock_path,
        canonical_root.join(".effigy/graph/catalogs/bovine/refresh.lock")
    );

    run_index_in_scope(&bovine).expect("index independent catalog");
    assert!(paths.db_path.is_file(), "independent database missing");
    assert!(
        !canonical_root.join(".effigy/graph/graph.db").exists(),
        "independent indexing wrote the shared database"
    );
    assert!(
        !canonical_root
            .join(".effigy/graph/catalogs/farmyard/graph.db")
            .exists(),
        "independent indexing touched a sibling database"
    );

    let _ = fs::remove_dir_all(&fixture.root);
}

fn selection_fixture(prefix: &str) -> (PathBuf, Vec<effigy_manifest::LoadedCatalog>) {
    let root = std::env::temp_dir().join(format!(
        "{prefix}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(root.join("apps/bovine/nested")).expect("root");
    loaded_catalog(
        "bovine",
        &root.join("apps/bovine"),
        "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nsegmented = true\n",
        1,
    );
    loaded_catalog(
        "desktop",
        &root.join("apps/bovine/nested"),
        "[catalog]\nalias = \"desktop\"\n\n[catalog.graph]\nsegmented = true\n",
        2,
    );
    loaded_catalog(
        "folded",
        &root.join("apps/folded"),
        "[catalog]\nalias = \"folded\"\n",
        1,
    );
    loaded_catalog("root", &root, "[catalog]\nalias = \"root\"\n", 0);
    let catalogs = vec![
        loaded_catalog(
            "bovine",
            &root.join("apps/bovine"),
            &fs::read_to_string(root.join("apps/bovine/effigy.toml")).expect("bovine"),
            1,
        ),
        loaded_catalog(
            "desktop",
            &root.join("apps/bovine/nested"),
            &fs::read_to_string(root.join("apps/bovine/nested/effigy.toml")).expect("desktop"),
            2,
        ),
        loaded_catalog(
            "folded",
            &root.join("apps/folded"),
            &fs::read_to_string(root.join("apps/folded/effigy.toml")).expect("folded"),
            1,
        ),
        loaded_catalog(
            "root",
            &root,
            &fs::read_to_string(root.join("effigy.toml")).expect("root"),
            0,
        ),
    ];
    (root, catalogs)
}

#[test]
fn selection_prefers_cwd_deepest_catalog_and_explicit_wins() {
    let (root, catalogs) = selection_fixture("graph-selection");
    let cwd = root.join("apps/bovine/nested/src");
    fs::create_dir_all(&cwd).expect("cwd");

    let plan = select_scopes(&root, &catalogs, &GraphScopeRequest::Cwd, &cwd).expect("cwd plan");
    let GraphScopePlan::Single(scope) = plan else {
        panic!("cwd selection must not fan out");
    };
    assert_eq!(scope.alias(), "desktop");
    assert_eq!(scope.selection(), crate::scope::GraphScopeSelection::Cwd);

    let plan = select_scopes(
        &root,
        &catalogs,
        &GraphScopeRequest::Catalog("bovine".to_owned()),
        &cwd,
    )
    .expect("explicit plan");
    let GraphScopePlan::Single(scope) = plan else {
        panic!("explicit selection must not fan out");
    };
    assert_eq!(scope.alias(), "bovine");
    assert_eq!(
        scope.selection(),
        crate::scope::GraphScopeSelection::Explicit
    );

    // Explicit selection overrides a deeper cwd match.
    assert_ne!(scope.alias(), "desktop");

    let plan =
        select_scopes(&root, &catalogs, &GraphScopeRequest::AllCatalogs, &root).expect("fan out");
    let GraphScopePlan::FanOut(scopes) = plan else {
        panic!("fan-out plan expected");
    };
    let aliases = scopes.iter().map(|scope| scope.alias()).collect::<Vec<_>>();
    assert_eq!(aliases, vec!["root", "bovine", "desktop"]);

    let error = select_scopes(
        &root,
        &catalogs,
        &GraphScopeRequest::Catalog("unknown".to_owned()),
        &root,
    )
    .expect_err("unknown alias must fail");
    assert!(error.to_string().contains("unknown catalog `unknown`"));
    assert!(error.to_string().contains("bovine"));

    let error = select_scopes(
        &root,
        &catalogs,
        &GraphScopeRequest::Catalog("folded".to_owned()),
        &root,
    )
    .expect_err("folded alias must fail");
    assert!(error.to_string().contains("folded"));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unsafe_topology_fails_before_any_graph_state() {
    // independent without segmented is rejected by manifest validation.
    let manifest_body = "[catalog]\nalias = \"bovine\"\n\n[catalog.graph]\nindependent = true\n";
    let manifest: effigy_manifest::TaskManifest =
        toml::from_str(manifest_body).expect("parse manifest");
    let path = std::env::temp_dir().join("graph-unsafe-independent/effigy.toml");
    let error = manifest
        .validate(&path)
        .expect_err("independent without segmented must fail");
    assert!(
        error.to_string().contains("requires `segmented = true`"),
        "{error}"
    );

    // A segmented catalog that escapes the workspace root is rejected.
    let workspace = std::env::temp_dir().join(format!(
        "graph-unsafe-escape-ws-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let outside = std::env::temp_dir().join(format!(
        "graph-unsafe-escape-out-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&workspace).expect("ws");
    fs::create_dir_all(&outside).expect("outside");
    loaded_catalog(
        "external",
        &outside,
        "[catalog]\nalias = \"external\"\n\n[catalog.graph]\nsegmented = true\n",
        1,
    );
    let catalogs = vec![
        loaded_catalog("root", &workspace, "[catalog]\nalias = \"root\"\n", 0),
        loaded_catalog(
            "external",
            &outside,
            &fs::read_to_string(outside.join("effigy.toml")).expect("external"),
            1,
        ),
    ];
    let error = build_scopes(&workspace, &catalogs).expect_err("escaping root must fail");
    assert!(error.to_string().contains("outside the workspace root"));

    let _ = fs::remove_dir_all(&workspace);
    let _ = fs::remove_dir_all(&outside);
}

#[test]
fn external_reference_stays_unresolved_without_sibling_refresh() {
    let fixture = two_catalog_fixture("graph-relation-boundary", true);
    write_source(
        &fixture.root.join("apps/bovine/src/lib.rs"),
        "pub fn bovine_uses() { farmyard_symbol(); }\n",
    );
    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");
    let farmyard = scope_by_alias(&scopes, "farmyard");

    run_index_in_scope(&bovine).expect("index bovine");

    // The cross-catalog reference is a boundary, not a reason to open or
    // refresh the sibling's independent database.
    assert!(
        !farmyard.paths().db_path.exists(),
        "resolving an external reference opened a sibling database"
    );
    let payload =
        crate::callers_in_scope(&bovine, "symbol:rust:bovine_uses", Some(10)).expect("callers");
    assert!(
        payload
            .nodes
            .iter()
            .all(|symbol| symbol.provenance.source_path.starts_with("apps/bovine")),
        "callers escaped the selected scope"
    );

    // Indexing the sibling later does not retroactively widen the selected
    // scope's stored results until that scope is reindexed.
    run_index_in_scope(&farmyard).expect("index farmyard");
    let farmyard_store = GraphStore::open_for_scope(&farmyard).expect("farmyard store");
    let farmyard_files = farmyard_store
        .list_files_in_scope(&farmyard)
        .expect("farmyard files");
    assert!(!farmyard_files.is_empty());
    let bovine_store = GraphStore::open_for_scope(&bovine).expect("bovine store");
    assert!(bovine_store
        .list_files_in_scope(&bovine)
        .expect("bovine files")
        .iter()
        .all(|file| file.path.starts_with("apps/bovine")));

    let _ = fs::remove_dir_all(&fixture.root);
}

fn git(repo_root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn sibling_dirty_tree_does_not_defeat_a_scoped_git_gate() {
    let fixture = two_catalog_fixture("graph-scoped-git", false);
    git(&fixture.root, &["init", "-q", "-b", "main"]);
    git(&fixture.root, &["add", "-A"]);
    git(
        &fixture.root,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );

    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");
    run_index_in_scope(&bovine).expect("index bovine");

    let store = GraphStore::open_for_scope(&bovine).expect("store");
    assert!(
        crate::git::git_gate_says_fresh(&bovine, &store).expect("gate"),
        "clean scoped tree must satisfy the gate"
    );

    // A sibling edit must not defeat this scope's gate: the scoped status
    // query never looks outside the selected catalog.
    write_source(
        &fixture.root.join("apps/farmyard/src/lib.rs"),
        "pub fn farmyard_changed() {}\n",
    );
    assert!(
        crate::git::git_gate_says_fresh(&bovine, &store).expect("gate"),
        "a dirty sibling defeated the scoped gate"
    );

    // An edit inside the scope must defeat it.
    write_source(
        &fixture.root.join("apps/bovine/src/lib.rs"),
        "pub fn bovine_changed() {}\n",
    );
    assert!(
        !crate::git::git_gate_says_fresh(&bovine, &store).expect("gate"),
        "a dirty scope kept its gate on"
    );

    let _ = fs::remove_dir_all(&fixture.root);
}

#[test]
fn graph_json_catalog_evidence_is_additive() {
    let fixture = two_catalog_fixture("graph-json-catalog", true);
    let scopes = build_scopes(&fixture.root, &catalogs_for(&fixture)).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");
    let payload = GraphCommandPayload::new(
        "effigy.graph.status.v1",
        "graph status",
        fixture.root.display().to_string(),
        serde_json::json!({"ready": true}),
    )
    .with_catalog(GraphCatalogPayload::from_scope(&bovine));
    let rendered: serde_json::Value =
        serde_json::from_str(&render_json(&payload, "{}")).expect("json");
    assert_eq!(rendered["catalog"]["alias"], "bovine");
    assert_eq!(rendered["catalog"]["root"], "apps/bovine");
    assert_eq!(rendered["catalog"]["segmented"], true);
    assert_eq!(rendered["catalog"]["independent"], true);
    assert_eq!(rendered["catalog"]["selection"], "root");
    assert_eq!(rendered["schema_version"], 1);
    let _ = fs::remove_dir_all(&fixture.root);
}

#[test]
fn docs_context_keeps_its_corpus_without_refreshing_catalog_scopes() {
    let fixture = two_catalog_fixture("graph-docs-corpus", true);
    write_source(
        &fixture.root.join("apps/bovine/docs/guide.md"),
        "# Bovine guide\n\nThe widget calibrator lives here.\n",
    );
    let catalogs = catalogs_for(&fixture);
    let scopes = build_scopes(&fixture.root, &catalogs).expect("scopes");
    let bovine = scope_by_alias(&scopes, "bovine");

    let payload = crate::docs_context(
        &fixture.root,
        "widget calibrator",
        crate::DocsContextRequest::default(),
    )
    .expect("docs context");
    assert!(
        payload
            .results
            .iter()
            .any(|result| result.path == "apps/bovine/docs/guide.md"),
        "configured documentation under a segmented catalog was pruned: {:?}",
        payload
            .results
            .iter()
            .map(|result| result.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        !bovine.paths().db_path.exists(),
        "docs context created an independent catalog database"
    );
    let _ = fs::remove_dir_all(&fixture.root);
}
