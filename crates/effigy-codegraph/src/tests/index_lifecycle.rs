use super::*;

#[test]
fn graph_index_and_query_cover_mixed_repo_fixture() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::create_dir_all(temp.path().join("web")).expect("mkdir web");
    fs::create_dir_all(temp.path().join("legacy")).expect("mkdir legacy");

    fs::write(
        temp.path().join("effigy.toml"),
        r#"[tasks.build]
run = "cargo test"
"#,
    )
    .expect("write manifest");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn run_release() {
    helper();
}

fn helper() {}
"#,
    )
    .expect("write rust");
    fs::write(
        temp.path().join("src/release_graph_helper_extra.rs"),
        r#"
pub fn release_graph_secondary_worker() {
    release_graph_helper();
}
"#,
    )
    .expect("write second rust");
    fs::write(
        temp.path().join("docs/README.md"),
        "# Release Guide\n\nSee [root manifest](../effigy.toml).\n",
    )
    .expect("write docs");
    fs::write(
        temp.path().join("web/index.ts"),
        "export function renderApp() { return helper(); }\nfunction helper() { return 1; }\n",
    )
    .expect("write ts");
    fs::write(
        temp.path().join("legacy/index.php"),
        "<?php\nfunction run_page() { require 'boot.php'; helper(); }\n",
    )
    .expect("write php");

    let report = run_index(temp.path()).expect("index");
    assert!(report.indexed_files >= 5);
    assert!(report.counts.symbols > 0);

    let status_payload = status(temp.path()).expect("status");
    assert!(status_payload.ready);
    assert_eq!(status_payload.freshness.state, "ready");
    assert!(status_payload.freshness.usable);
    assert!(status_payload.stale_paths.is_empty());

    let files_payload = query_files(temp.path(), None).expect("files");
    assert!(!files_payload.freshness.stale);
    assert_eq!(files_payload.freshness.state, "ready");
    assert!(files_payload
        .files
        .iter()
        .any(|file| file.path == "src/lib.rs"));
    assert!(files_payload
        .files
        .iter()
        .any(|file| file.path == "effigy.toml"));
    assert!(files_payload
        .files
        .iter()
        .any(|file| file.path == "docs/README.md"));

    let search_payload = query_search(temp.path(), "run", Some(10)).expect("search");
    assert!(!search_payload.freshness.stale);
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(!search_payload.matches.is_empty());

    let symbol_match = search_payload
        .matches
        .iter()
        .find(|entry| entry.record_type == "symbol")
        .expect("symbol match");
    let node_payload = node(temp.path(), &symbol_match.record_id).expect("node");
    assert!(node_payload.symbol.is_some());

    let callers_payload = callers(temp.path(), &symbol_match.record_id, Some(10)).expect("callers");
    assert!(callers_payload.edges.len() <= 10);

    let impact_payload = impact(temp.path(), "src/lib.rs", Some(20)).expect("impact");
    assert_eq!(impact_payload.files.len(), 1);
    assert!(!impact_payload.symbols.is_empty());

    let context_payload = context(
        temp.path(),
        "trace release helper",
        Some(4),
        Some(2048),
        &[],
        &[],
    )
    .expect("context");
    assert!(!context_payload.freshness.stale);
    assert_eq!(context_payload.freshness.state, "ready");
    assert!(!context_payload.items.is_empty());
    assert!(context_payload
        .items
        .iter()
        .all(|item| !item.reasons.is_empty()));
    assert!(context_payload.overflow.byte_budget >= context_payload.overflow.used_bytes);

    let explore_payload = explore(
        temp.path(),
        "trace release helper",
        Some(3),
        Some(4096),
        &[],
        &[],
    )
    .expect("explore");
    assert!(!explore_payload.index.freshness.stale);
    assert_eq!(explore_payload.index.freshness.state, "ready");
    assert!(!explore_payload.primary.is_empty());
    assert!(!explore_payload.excerpts.is_empty());
    assert!(explore_payload
        .guidance
        .iter()
        .any(|note| note.contains("use `rg`")));
}

#[test]
fn graph_status_reports_changed_paths_as_stale() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    run_index(temp.path()).expect("index");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    let payload = status(temp.path()).expect("status");
    assert!(payload.stale_paths.contains(&"src/lib.rs".to_owned()));
    assert!(payload.changed_paths.contains(&"src/lib.rs".to_owned()));
    assert_eq!(payload.freshness.state, "refresh-recommended");
    assert!(payload.freshness.usable);
    assert_eq!(payload.freshness.stale_path_count, 1);

    // Queries refresh a stale index on demand instead of returning stale data;
    // `status` stays report-only.
    let search_payload = query_search(temp.path(), "release", Some(10)).expect("search");
    assert!(!search_payload.freshness.stale);
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload.freshness.stale_paths.is_empty());
    assert!(search_payload
        .freshness
        .summary
        .contains("graph auto-refreshed"));
}

#[test]
fn graph_status_without_index_reports_missing_index_trust_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    let payload = status(temp.path()).expect("status");
    assert!(!payload.ready);
    assert_eq!(payload.freshness.state, "missing-index");
    assert!(!payload.freshness.usable);
    assert!(payload
        .freshness
        .summary
        .contains("effigy graph index --json"));
    assert!(payload.freshness.stale);
    assert_eq!(payload.freshness.stale_path_count, 1);
    assert!(payload.stale_paths.contains(&"src/lib.rs".to_owned()));
}

#[test]
fn graph_index_reuses_unchanged_content_when_only_mtime_moves() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    let first = run_index(temp.path()).expect("first index");
    assert_eq!(first.failed_paths.len(), 0);

    std::thread::sleep(std::time::Duration::from_millis(5));
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite same rust");

    let second = run_index(temp.path()).expect("second index");
    assert_eq!(second.failed_paths.len(), 0);
    assert!(second.changed_paths.contains(&"src/lib.rs".to_owned()));

    let status_payload = status(temp.path()).expect("status");
    assert!(status_payload.stale_paths.is_empty());
    assert_eq!(status_payload.freshness.state, "ready");

    let store = GraphStore::open(temp.path()).expect("open store");
    assert_eq!(store.list_index_runs().expect("runs").len(), 2);
    assert_eq!(store.list_files().expect("files").len(), 1);
    assert_eq!(store.list_symbols().expect("symbols").len(), 2);
}

#[test]
fn graph_index_removes_deleted_file_records_on_reindex() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    fs::write(
        temp.path().join("docs/README.md"),
        "# Release Guide\n\nSee src/lib.rs.\n",
    )
    .expect("write markdown");

    let first = run_index(temp.path()).expect("first index");
    assert_eq!(first.failed_paths.len(), 0);

    fs::remove_file(temp.path().join("docs/README.md")).expect("remove markdown");

    let second = run_index(temp.path()).expect("second index");
    assert_eq!(second.failed_paths.len(), 0);
    assert!(second.deleted_paths.contains(&"docs/README.md".to_owned()));

    let files_payload = query_files(temp.path(), None).expect("files");
    assert!(!files_payload
        .files
        .iter()
        .any(|file| file.path == "docs/README.md"));

    let search_payload = query_search(temp.path(), "Guide", Some(10)).expect("search");
    assert!(search_payload.matches.is_empty());
}
