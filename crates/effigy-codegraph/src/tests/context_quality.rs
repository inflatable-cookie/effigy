use super::*;

#[test]
fn graph_context_enforces_byte_budget_and_reports_overflow() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn release_graph_helper() {
    let payload = "release graph helper release graph helper";
    println!("{payload}");
}

pub fn release_graph_worker() {
    release_graph_helper();
}

pub fn release_graph_worker_two() {
    release_graph_helper();
}

pub fn release_graph_worker_three() {
    release_graph_helper();
}

pub fn release_graph_worker_four() {
    release_graph_helper();
}
"#,
    )
    .expect("write rust");
    fs::write(
        temp.path().join("docs/README.md"),
        "# Release Graph Helper\n\nThis document traces the release graph helper flow.\n",
    )
    .expect("write docs");

    run_index(temp.path()).expect("index");
    let payload = context(
        temp.path(),
        "release graph helper",
        Some(1),
        Some(32),
        &[],
        &[],
    )
    .expect("context");

    assert!(!payload.items.is_empty());
    assert!(payload.overflow.used_bytes <= payload.overflow.byte_budget);
    assert!(
        payload.overflow.omitted_items > 0
            || payload.overflow.omitted_files > 0
            || payload.overflow.omitted_symbols > 0
    );
    assert!(payload
        .items
        .iter()
        .any(|item| item.snippet_truncated || item.snippet.is_none()));
    assert!(payload
        .notes
        .iter()
        .any(|note| note.starts_with("byte budget: ")));
}

#[test]
fn graph_context_ranks_implementation_before_tests_for_implementation_requests() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src/graph")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("tests")).expect("mkdir tests");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::write(
        temp.path().join("src/graph/watch.rs"),
        r#"
pub fn watch_repo() {
    refresh_graph_index();
}

fn refresh_graph_index() {}
"#,
    )
    .expect("write implementation");
    fs::write(
        temp.path().join("tests/graph_watch_tests.rs"),
        r#"
fn graph_watch_streams_started_event() {}
fn graph_watch_streams_refresh_event() {}
fn graph_watch_reconciles_dirty_event() {}
fn graph_watch_reports_backend_error() {}
fn graph_watch_keeps_index_fresh() {}
"#,
    )
    .expect("write tests");
    fs::write(
        temp.path().join("docs/graph-watch.md"),
        "# Graph Watch\n\nUse graph watch to keep the graph index fresh.\n",
    )
    .expect("write docs");

    run_index(temp.path()).expect("index");
    let payload = context(
        temp.path(),
        "trace graph watch implementation",
        Some(4),
        Some(4096),
        &["rust".to_owned()],
        &[],
    )
    .expect("context");

    let ranked_paths = payload
        .items
        .iter()
        .filter(|item| item.kind == "file")
        .map(|item| item.path.as_str())
        .collect::<Vec<_>>();

    assert!(
        ranked_paths.contains(&"src/graph/watch.rs"),
        "implementation file should be in the context set: {ranked_paths:?}"
    );
    assert_eq!(
        ranked_paths.first().copied(),
        Some("src/graph/watch.rs"),
        "implementation intent should rank implementation before tests: {ranked_paths:?}"
    );
}

#[test]
fn graph_context_implementation_requests_do_not_rank_comment_only_matches_first() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("crates/effigy-release/src")).expect("mkdir release src");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
//! Release orchestration overview.
//! This comment links readers to release orchestration docs.

pub fn unrelated_root_library_entrypoint() {}
"#,
    )
    .expect("write root lib");
    fs::write(
        temp.path().join("crates/effigy-release/src/lib.rs"),
        r#"
pub fn release_orchestration_prepare() {
    release_orchestration_execute();
}

fn release_orchestration_execute() {}
"#,
    )
    .expect("write release lib");

    run_index(temp.path()).expect("index");
    let payload = context(
        temp.path(),
        "understand release orchestration",
        Some(3),
        Some(4096),
        &[],
        &[],
    )
    .expect("context");

    assert_eq!(
        payload.items.first().map(|item| item.path.as_str()),
        Some("crates/effigy-release/src/lib.rs"),
        "implementation intent should prefer executable source evidence over Rust doc comments: {:?}",
        payload
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_context_maps_task_route_language_to_selector_parsing() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("crates/effigy-tasks/src")).expect("mkdir tasks src");
    fs::create_dir_all(temp.path().join("crates/noise/src")).expect("mkdir noise src");
    fs::write(
        temp.path().join("crates/effigy-tasks/src/parsing.rs"),
        r#"
pub fn parse_task_selector(selector: &str) -> TaskSelector {
    TaskSelector { raw: selector.to_owned() }
}

pub struct TaskSelector {
    pub raw: String,
}
"#,
    )
    .expect("write task parsing");
    fs::write(
        temp.path().join("crates/noise/src/parser.rs"),
        r#"
pub fn parse_task_log_line(line: &str) -> String {
    let parsed = line.trim();
    parsed.to_owned()
}
"#,
    )
    .expect("write noise parser");

    run_index(temp.path()).expect("index");
    let payload = context(
        temp.path(),
        "where are task routes parsed",
        Some(4),
        Some(4096),
        &[],
        &[],
    )
    .expect("context");

    assert_eq!(
        payload.items.first().map(|item| item.path.as_str()),
        Some("crates/effigy-tasks/src/parsing.rs"),
        "task route language should resolve toward task selector parsing: {:?}",
        payload
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_context_ranks_tests_and_docs_when_request_intent_asks_for_them() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_graph_watch_fixture(temp.path());

    run_index(temp.path()).expect("index");

    let test_payload = context(
        temp.path(),
        "graph watch regression tests",
        Some(4),
        Some(4096),
        &[],
        &[],
    )
    .expect("test context");
    assert_eq!(
        test_payload.items.first().map(|item| item.path.as_str()),
        Some("tests/graph_watch_tests.rs"),
        "test intent should rank tests first: {:?}",
        test_payload
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );

    let docs_payload = context(
        temp.path(),
        "docs graph watch guide",
        Some(4),
        Some(4096),
        &[],
        &[],
    )
    .expect("docs context");
    assert_eq!(
        docs_payload.items.first().map(|item| item.path.as_str()),
        Some("docs/graph-watch.md"),
        "docs intent should rank docs first: {:?}",
        docs_payload
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_context_file_snippets_start_near_matched_symbol_evidence() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn unrelated_header() {
    println!("header");
}

pub fn deploy_provider_export_owner() {
    write_provider_files();
}

fn write_provider_files() {}
"#,
    )
    .expect("write rust");

    run_index(temp.path()).expect("index");
    let payload = context(
        temp.path(),
        "trace deploy provider export",
        Some(1),
        Some(4096),
        &["rust".to_owned()],
        &[],
    )
    .expect("context");
    let first = payload.items.first().expect("context item");

    assert_eq!(first.path, "src/lib.rs");
    assert_eq!(first.range.as_ref().map(|range| range.start.line), Some(6));
    let snippet = first.snippet.as_deref().expect("snippet");
    assert!(
        snippet.contains("deploy_provider_export_owner"),
        "snippet should include matched symbol evidence: {snippet}"
    );
    assert!(
        !snippet.contains("unrelated_header"),
        "snippet should not start at the file header when symbol evidence exists: {snippet}"
    );
}

#[test]
fn graph_search_returns_actionable_symbol_snippets() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn release_search_owner() {
    emit_release_report();
}

fn emit_release_report() {}
"#,
    )
    .expect("write rust");

    run_index(temp.path()).expect("index");
    let payload = query_search(temp.path(), "release_search_owner", Some(5)).expect("search");
    let symbol_match = payload
        .matches
        .iter()
        .find(|item| item.record_type == "symbol")
        .expect("symbol match");

    assert_eq!(symbol_match.path.as_deref(), Some("src/lib.rs"));
    assert_eq!(symbol_match.name.as_deref(), Some("release_search_owner"));
    assert!(symbol_match
        .snippet
        .as_deref()
        .is_some_and(|snippet| snippet.contains("release_search_owner")));
}

#[test]
fn graph_store_source_search_indexes_file_bodies_without_leaking_into_public_search() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn release_orchestration_prepare() {
    release_orchestration_execute();
}

fn release_orchestration_execute() {}
"#,
    )
    .expect("write rust");

    run_index(temp.path()).expect("index");
    let store = GraphStore::open(temp.path()).expect("open store");

    let source_matches = store
        .source_search("orchestration", 10)
        .expect("source search");
    assert!(
        source_matches
            .iter()
            .any(|item| item.file_id.as_str() == "file:src/lib.rs"),
        "source search should index file bodies: {source_matches:?}"
    );

    let public_search = query_search(temp.path(), "orchestration", Some(10)).expect("search");
    assert!(
        public_search
            .matches
            .iter()
            .all(|item| item.record_type != "source"),
        "public graph search should not leak internal source rows: {:?}",
        public_search
            .matches
            .iter()
            .map(|item| item.record_type.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_explore_traverses_import_neighbors_and_emits_related_file_excerpts() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("web/components")).expect("mkdir components");
    fs::write(
        temp.path().join("web/util.ts"),
        "export function helper() { return 1; }\n",
    )
    .expect("write util");
    fs::write(
        temp.path().join("web/components/Button.tsx"),
        r#"import React from "react";
import { helper } from "../util";

export const Button = () => <button>{helper()}</button>;
"#,
    )
    .expect("write button");

    run_index(temp.path()).expect("index");
    let payload = explore(
        temp.path(),
        "trace button helper flow",
        Some(1),
        Some(4096),
        &["tsx".to_owned(), "typescript".to_owned()],
        &[],
    )
    .expect("explore");

    let related_paths = payload
        .relations
        .iter()
        .map(|item| item.path.as_str())
        .collect::<Vec<_>>();

    assert!(
        payload.relations.iter().any(|item| {
            item.path == "web/util.ts"
                && (item.reason.contains("import") || item.reason.contains("call"))
        }),
        "traversal should add a related util neighbor file from import or call flow: {:?}",
        payload
            .relations
            .iter()
            .map(|item| format!("{} => {}", item.path, item.reason))
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .excerpts
            .iter()
            .any(|item| item.path == "web/util.ts" && item.text.contains("helper")),
        "traversal should add a related file excerpt: {:?}",
        payload
            .excerpts
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        related_paths.contains(&"web/util.ts"),
        "expected util traversal relation: {related_paths:?}"
    );
}

#[test]
fn graph_explore_traverses_unresolved_rust_call_neighbors() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/helper.rs"),
        r#"
pub fn release_graph_helper() -> &'static str {
    "ok"
}
"#,
    )
    .expect("write helper");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
mod helper;

pub fn release_graph_worker() -> &'static str {
    helper::release_graph_helper()
}
"#,
    )
    .expect("write lib");

    run_index(temp.path()).expect("index");
    let payload = explore(
        temp.path(),
        "trace release graph worker helper flow",
        Some(1),
        Some(4096),
        &["rust".to_owned()],
        &[],
    )
    .expect("explore");

    assert_eq!(
        payload.primary.first().map(|item| item.path.as_str()),
        Some("src/lib.rs"),
        "worker file should stay primary so traversal has to reach helper: {:?}",
        payload
            .primary
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .relations
            .iter()
            .any(|item| item.path == "src/helper.rs"),
        "traversal should resolve unresolved Rust call edges into helper neighbors: {:?}",
        payload
            .relations
            .iter()
            .map(|item| format!("{} => {}", item.path, item.reason))
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .excerpts
            .iter()
            .any(|item| item.path == "src/helper.rs" && item.text.contains("release_graph_helper")),
        "traversal should append helper excerpts for unresolved Rust call edges: {:?}",
        payload
            .excerpts
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_index_handles_multiline_rust_impl_targets_without_id_failures() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub struct Example<T> {
    value: T,
}

impl<T> Example<
    T,
>
where
    T: Clone,
{
    pub fn clone_value(&self) -> T {
        self.value.clone()
    }
}
"#,
    )
    .expect("write rust");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 0);
}

#[test]
fn graph_index_skips_empty_markdown_headings_without_diagnostics() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::write(
        temp.path().join("docs/README.md"),
        "# Title\n\n## \n\n### ---\n\nSee [manifest](../effigy.toml).\n",
    )
    .expect("write markdown");
    fs::write(
        temp.path().join("effigy.toml"),
        "[tasks.test]\nrun = \"cargo test\"\n",
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 0);
}
