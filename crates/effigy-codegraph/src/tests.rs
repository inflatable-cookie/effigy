use crate::json::{
    render_json, GraphCommandPayload, GraphContextItemPayload, GraphContextOverflowPayload,
    GraphContextPayload, GraphCountsPayload, GraphExploreExcerptPayload, GraphExploreIndexPayload,
    GraphExplorePayload, GraphExploreRelationPayload, GraphStatusPayload,
};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, ExtractorCapability,
    ExtractorRecord, FileIndexStatus, FileRecord, IndexRunRecord, Provenance, ReferenceRecord,
    SourcePosition, SourceSpan, SymbolRecord, GRAPH_STORAGE_SCHEMA_VERSION,
};
use crate::{
    affected, callers, context, explore, impact, node, query_files, query_search, run_index,
    status, CodeGraphError, ExtractorId, GraphId, GraphStore, GRAPH_JSON_SCHEMA_VERSION,
};
use rusqlite::Connection;
use std::fs;
use std::path::Path;

fn span() -> SourceSpan {
    SourceSpan {
        start: SourcePosition {
            line: 1,
            column: 0,
            byte: 0,
        },
        end: SourcePosition {
            line: 1,
            column: 10,
            byte: 10,
        },
    }
}

fn provenance() -> Provenance {
    Provenance {
        extractor_id: ExtractorId::new("rust").expect("extractor id"),
        extractor_version: "0.1.0".to_owned(),
        source_path: "src/lib.rs".to_owned(),
        confidence: Confidence::Syntactic,
        detail: Some("tree-sitter pass".to_owned()),
    }
}

fn write_graph_watch_fixture(root: &Path) {
    fs::create_dir_all(root.join("src/graph")).expect("mkdir src");
    fs::create_dir_all(root.join("tests")).expect("mkdir tests");
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::write(
        root.join("src/graph/watch.rs"),
        "pub fn watch_repo() { refresh_graph_index(); }\nfn refresh_graph_index() {}\n",
    )
    .expect("write implementation");
    fs::write(
        root.join("tests/graph_watch_tests.rs"),
        "fn graph_watch_regression_test() {}\nfn graph_watch_coverage_test() {}\n",
    )
    .expect("write tests");
    fs::write(
        root.join("docs/graph-watch.md"),
        "# Graph Watch Guide\n\nDocs for graph watch agent workflow.\n",
    )
    .expect("write docs");
}

fn write_php_front_controller_fixture(root: &Path) {
    fs::create_dir_all(root.join("legacy/App")).expect("mkdir app");
    fs::write(
        root.join("legacy/boot.php"),
        "<?php\nconst BOOTSTRAPPED = true;\n",
    )
    .expect("write boot");
    fs::write(
        root.join("legacy/index.php"),
        r#"<?php
require_once 'boot.php';
App\Controller\HomeController::handle();
"#,
    )
    .expect("write front controller");
    fs::write(
        root.join("legacy/App/Controller.php"),
        r#"<?php
namespace App\Controller;

use Legacy\Support\Helper;

trait UsesHelper {
    public function helperName() {
        return Helper::name();
    }
}

interface Renderable {
    public function render();
}

class HomeController implements Renderable {
    use UsesHelper;

    public const VERSION = '1.0';

    public static function handle() {
        require_once __DIR__ . '/../boot.php';
        $instance = new self();
        $instance->render();
    }

    public function render() {
        echo $this->helperName();
    }
}
"#,
    )
    .expect("write controller");
}

#[test]
fn graph_ids_reject_invalid_tokens() {
    assert!(GraphId::new("symbol::ok").is_ok());
    assert!(matches!(
        GraphId::new(""),
        Err(CodeGraphError::Validation(_))
    ));
    assert!(matches!(
        GraphId::new("bad id"),
        Err(CodeGraphError::Validation(_))
    ));
}

#[test]
fn graph_store_initializes_graph_dir_and_reopens() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(temp.path()).expect("open");
    assert_eq!(
        store.storage_schema_version().expect("schema version"),
        GRAPH_STORAGE_SCHEMA_VERSION
    );
    assert!(store.paths().graph_dir.is_dir());
    assert!(store.paths().db_path.is_file());
    assert!(store.search_table_present().expect("fts table"));

    let reopened = GraphStore::open(temp.path()).expect("reopen");
    assert_eq!(
        reopened.storage_schema_version().expect("schema version"),
        GRAPH_STORAGE_SCHEMA_VERSION
    );
    let journal_mode = reopened.journal_mode().expect("journal mode");
    assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
}

#[test]
fn graph_store_migrates_v1_search_index_to_source_backfill() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn helper() { println!(\"hello scale\"); }\n",
    )
    .expect("write source");

    let graph_dir = temp.path().join(".effigy/graph");
    fs::create_dir_all(&graph_dir).expect("graph dir");
    let db_path = graph_dir.join("graph.db");
    let connection = Connection::open(&db_path).expect("open sqlite");
    connection
        .execute_batch(
            "
            CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE extractors (
                id TEXT PRIMARY KEY,
                version TEXT NOT NULL,
                languages_json TEXT NOT NULL,
                capabilities_json TEXT NOT NULL
            );
            CREATE TABLE index_runs (
                id TEXT PRIMARY KEY,
                repo_root TEXT NOT NULL,
                schema_version INTEGER NOT NULL,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                file_count INTEGER NOT NULL,
                symbol_count INTEGER NOT NULL,
                edge_count INTEGER NOT NULL
            );
            CREATE TABLE files (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                content_hash TEXT NOT NULL,
                language_id TEXT NOT NULL,
                byte_size INTEGER NOT NULL,
                status TEXT NOT NULL
            );
            CREATE TABLE file_scan_state (
                path TEXT PRIMARY KEY,
                content_hash TEXT NOT NULL,
                language_id TEXT NOT NULL,
                modified_unix_ms TEXT NOT NULL,
                byte_size INTEGER NOT NULL
            );
            CREATE TABLE symbols (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                display_name TEXT NOT NULL,
                canonical_name TEXT NOT NULL,
                file_id TEXT NOT NULL,
                span_json TEXT NOT NULL,
                provenance_json TEXT NOT NULL
            );
            CREATE TABLE edges (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                from_id TEXT NOT NULL,
                to_id TEXT,
                unresolved_target TEXT,
                provenance_json TEXT NOT NULL
            );
            CREATE TABLE graph_references (
                id TEXT PRIMARY KEY,
                file_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                target_id TEXT,
                unresolved_target TEXT,
                span_json TEXT NOT NULL,
                provenance_json TEXT NOT NULL
            );
            CREATE TABLE diagnostics (
                id TEXT PRIMARY KEY,
                severity TEXT NOT NULL,
                message TEXT NOT NULL,
                file_id TEXT,
                span_json TEXT,
                provenance_json TEXT NOT NULL
            );
            CREATE VIRTUAL TABLE graph_search USING fts5(record_type, record_id, text);
            ",
        )
        .expect("create legacy schema");
    connection
        .execute(
            "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
            ("storage_schema_version", "1"),
        )
        .expect("schema metadata");
    connection
        .execute(
            "INSERT INTO files (id, path, content_hash, language_id, byte_size, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                "file:src/lib.rs",
                "src/lib.rs",
                "abc123",
                "rust",
                42_i64,
                "\"indexed\"",
            ),
        )
        .expect("file");
    connection
        .execute(
            "INSERT INTO symbols (id, kind, display_name, canonical_name, file_id, span_json, provenance_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                "symbol:rust:crate::helper",
                "function",
                "helper",
                "crate::helper",
                "file:src/lib.rs",
                serde_json::to_string(&span()).expect("span json"),
                serde_json::to_string(&provenance()).expect("prov json"),
            ),
        )
        .expect("symbol");
    connection
        .execute(
            "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
            ("file", "file:src/lib.rs", "src/lib.rs"),
        )
        .expect("file search");
    connection
        .execute(
            "INSERT INTO graph_search (record_type, record_id, text) VALUES (?1, ?2, ?3)",
            (
                "symbol",
                "symbol:rust:crate::helper",
                "helper crate::helper",
            ),
        )
        .expect("symbol search");
    drop(connection);

    let store = GraphStore::open(temp.path()).expect("open migrated store");
    assert_eq!(
        store.storage_schema_version().expect("schema version"),
        GRAPH_STORAGE_SCHEMA_VERSION
    );
    assert_eq!(
        store
            .source_search("hello", 10)
            .expect("source search")
            .len(),
        1
    );
}

#[test]
fn graph_store_rejects_newer_storage_schema() {
    let temp = tempfile::tempdir().expect("tempdir");
    let graph_dir = temp.path().join(".effigy/graph");
    fs::create_dir_all(&graph_dir).expect("graph dir");
    let db_path = graph_dir.join("graph.db");
    let connection = Connection::open(&db_path).expect("open sqlite");
    connection
        .execute_batch(
            "
            CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE VIRTUAL TABLE graph_search USING fts5(record_type, record_id, text);
            ",
        )
        .expect("create schema");
    connection
        .execute(
            "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
            (
                "storage_schema_version",
                (GRAPH_STORAGE_SCHEMA_VERSION + 1).to_string(),
            ),
        )
        .expect("schema metadata");
    drop(connection);

    let error = GraphStore::open(temp.path())
        .err()
        .expect("future schema should fail");
    assert!(
        error.to_string().contains("newer than supported schema"),
        "unexpected error: {error}"
    );
}

#[test]
fn graph_store_round_trips_records_and_counts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(temp.path()).expect("open");

    let extractor = ExtractorRecord {
        id: ExtractorId::new("rust").expect("extractor id"),
        version: "0.1.0".to_owned(),
        language_ids: vec!["rust".to_owned()],
        capabilities: vec![
            ExtractorCapability::Symbols,
            ExtractorCapability::References,
        ],
    };
    store.save_extractor(&extractor).expect("save extractor");

    let file = FileRecord {
        id: GraphId::new("file:src/lib.rs").expect("file id"),
        path: "src/lib.rs".to_owned(),
        content_hash: "abc123".to_owned(),
        language_id: "rust".to_owned(),
        byte_size: 128,
        status: FileIndexStatus::Indexed,
    };
    store.save_file(&file).expect("save file");

    let symbol = SymbolRecord {
        id: GraphId::new("symbol:crate::run").expect("symbol id"),
        kind: "function".to_owned(),
        display_name: "run".to_owned(),
        canonical_name: "crate::run".to_owned(),
        file_id: file.id.clone(),
        span: span(),
        provenance: provenance(),
    };
    store.save_symbol(&symbol).expect("save symbol");

    let edge = EdgeRecord {
        id: GraphId::new("edge:call:run->helper").expect("edge id"),
        kind: "call".to_owned(),
        from_id: symbol.id.clone(),
        to_id: None,
        unresolved_target: Some("helper".to_owned()),
        provenance: provenance(),
    };
    store.save_edge(&edge).expect("save edge");

    let reference = ReferenceRecord {
        id: GraphId::new("ref:run:helper").expect("reference id"),
        file_id: file.id.clone(),
        kind: "call-site".to_owned(),
        target_id: Some(symbol.id.clone()),
        unresolved_target: None,
        span: span(),
        provenance: provenance(),
    };
    store.save_reference(&reference).expect("save reference");

    let diagnostic = DiagnosticRecord {
        id: GraphId::new("diag:1").expect("diagnostic id"),
        severity: DiagnosticSeverity::Warning,
        message: "unresolved helper kept as heuristic edge".to_owned(),
        file_id: Some(file.id.clone()),
        span: Some(span()),
        provenance: provenance(),
    };
    store.save_diagnostic(&diagnostic).expect("save diagnostic");

    let run = IndexRunRecord {
        id: GraphId::new("run:2026-05-17T12:00:00Z").expect("run id"),
        repo_root: temp.path().display().to_string(),
        schema_version: GRAPH_STORAGE_SCHEMA_VERSION,
        started_at: "2026-05-17T12:00:00Z".to_owned(),
        finished_at: Some("2026-05-17T12:00:01Z".to_owned()),
        file_count: 1,
        symbol_count: 1,
        edge_count: 1,
    };
    store.save_index_run(&run).expect("save run");

    assert_eq!(
        store.list_extractors().expect("extractors"),
        vec![extractor]
    );
    assert_eq!(store.list_files().expect("files"), vec![file]);
    assert_eq!(store.list_symbols().expect("symbols"), vec![symbol]);
    assert_eq!(store.list_edges().expect("edges"), vec![edge]);
    assert_eq!(
        store.list_references().expect("references"),
        vec![reference]
    );
    assert_eq!(
        store.list_diagnostics().expect("diagnostics"),
        vec![diagnostic]
    );
    assert_eq!(store.list_index_runs().expect("runs"), vec![run]);

    assert_eq!(
        store.counts().expect("counts"),
        GraphCountsPayload {
            files: 1,
            symbols: 1,
            edges: 1,
            references: 1,
            diagnostics: 1,
            extractors: 1,
            index_runs: 1,
        }
    );
}

#[test]
fn graph_store_rejects_records_without_provenance() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = GraphStore::open(temp.path()).expect("open");

    let symbol = SymbolRecord {
        id: GraphId::new("symbol:broken").expect("symbol id"),
        kind: "function".to_owned(),
        display_name: "broken".to_owned(),
        canonical_name: "crate::broken".to_owned(),
        file_id: GraphId::new("file:src/lib.rs").expect("file id"),
        span: span(),
        provenance: Provenance {
            extractor_id: ExtractorId::new("rust").expect("extractor id"),
            extractor_version: "".to_owned(),
            source_path: "".to_owned(),
            confidence: Confidence::Heuristic,
            detail: None,
        },
    };

    assert!(matches!(
        store.save_symbol(&symbol),
        Err(CodeGraphError::Validation(_))
    ));
}

#[test]
fn graph_json_payloads_are_versioned() {
    let payload = GraphCommandPayload::new(
        "effigy.graph.status.v1",
        "graph status",
        "/tmp/repo",
        GraphStatusPayload {
            ready: true,
            index_present: true,
            db_path: "/tmp/repo/.effigy/graph/graph.db".to_owned(),
            storage_schema_version: GRAPH_STORAGE_SCHEMA_VERSION,
            counts: GraphCountsPayload {
                files: 1,
                symbols: 2,
                edges: 3,
                references: 4,
                diagnostics: 0,
                extractors: 1,
                index_runs: 1,
            },
            stale_paths: vec![],
            new_paths: vec![],
            changed_paths: vec![],
            deleted_paths: vec![],
            skipped_paths: vec![],
            failed_paths: vec![],
            extractors: vec![],
        },
    );

    let rendered = render_json(
        &payload,
        "{\"schema\":\"effigy.graph.status.v1\",\"schema_version\":1}",
    );
    let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("json");
    assert_eq!(parsed["schema"], "effigy.graph.status.v1");
    assert_eq!(parsed["schema_version"], GRAPH_JSON_SCHEMA_VERSION);
    assert_eq!(parsed["command"], "graph status");
    assert_eq!(parsed["repo_root"], "/tmp/repo");
}

#[test]
fn graph_context_payload_round_trips() {
    let payload = GraphCommandPayload::new(
        "effigy.graph.context.v1",
        "graph context",
        "/tmp/repo",
        GraphContextPayload {
            request: "trace deploy provider export".to_owned(),
            freshness: crate::json::GraphFreshnessPayload {
                stale: false,
                stale_paths: vec![],
            },
            items: vec![GraphContextItemPayload {
                kind: "file".to_owned(),
                record_id: "file:src/lib.rs".to_owned(),
                path: "src/lib.rs".to_owned(),
                language_id: Some("rust".to_owned()),
                name: Some("src/lib.rs".to_owned()),
                range: None,
                rank: 1,
                score: 7,
                reasons: vec!["path matches `deploy`".to_owned()],
                provenance: None,
                snippet: Some("pub fn deploy() {}".to_owned()),
                snippet_truncated: false,
            }],
            overflow: GraphContextOverflowPayload {
                omitted_items: 0,
                omitted_files: 0,
                omitted_symbols: 0,
                omitted_docs: 0,
                byte_budget: 4096,
                used_bytes: 18,
            },
            notes: vec!["bounded result".to_owned()],
        },
    );
    let json = serde_json::to_string(&payload).expect("serialize");
    let decoded: GraphCommandPayload<GraphContextPayload> =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.schema, "effigy.graph.context.v1");
    assert_eq!(decoded.schema_version, GRAPH_JSON_SCHEMA_VERSION);
    assert_eq!(decoded.payload.request, "trace deploy provider export");
}

#[test]
fn graph_explore_payload_round_trips() {
    let payload = GraphCommandPayload::new(
        "effigy.graph.explore.v1",
        "graph explore",
        "/tmp/repo",
        GraphExplorePayload {
            query: "trace graph watch implementation".to_owned(),
            index: GraphExploreIndexPayload {
                freshness: crate::json::GraphFreshnessPayload {
                    stale: false,
                    stale_paths: vec![],
                },
                counts: GraphCountsPayload {
                    files: 1,
                    symbols: 1,
                    edges: 0,
                    references: 0,
                    diagnostics: 0,
                    extractors: 1,
                    index_runs: 1,
                },
            },
            summary: "Query selected one primary owner.".to_owned(),
            primary: vec![GraphContextItemPayload {
                kind: "file".to_owned(),
                record_id: "file:src/lib.rs".to_owned(),
                path: "src/lib.rs".to_owned(),
                language_id: Some("rust".to_owned()),
                name: Some("src/lib.rs".to_owned()),
                range: None,
                rank: 1,
                score: 10,
                reasons: vec!["path matches `graph`".to_owned()],
                provenance: None,
                snippet: Some("pub fn watch_repo() {}".to_owned()),
                snippet_truncated: false,
            }],
            excerpts: vec![GraphExploreExcerptPayload {
                path: "src/lib.rs".to_owned(),
                language_id: Some("rust".to_owned()),
                name: Some("src/lib.rs".to_owned()),
                range: None,
                role: "file".to_owned(),
                section_kind: "context-window".to_owned(),
                completeness: "surrounding-context".to_owned(),
                score: 10,
                reasons: vec!["path matches `graph`".to_owned()],
                text: "pub fn watch_repo() {}".to_owned(),
                truncated: false,
            }],
            relations: vec![GraphExploreRelationPayload {
                kind: "symbol".to_owned(),
                path: "src/lib.rs".to_owned(),
                name: Some("crate::watch_repo".to_owned()),
                range: None,
                reason: "symbol matches `watch`".to_owned(),
            }],
            overflow: GraphContextOverflowPayload {
                omitted_items: 0,
                omitted_files: 0,
                omitted_symbols: 0,
                omitted_docs: 0,
                byte_budget: 4096,
                used_bytes: 22,
            },
            guidance: vec!["use `rg` for exact token verification".to_owned()],
        },
    );
    let json = serde_json::to_string(&payload).expect("serialize");
    let decoded: GraphCommandPayload<GraphExplorePayload> =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.schema, "effigy.graph.explore.v1");
    assert_eq!(decoded.schema_version, GRAPH_JSON_SCHEMA_VERSION);
    assert_eq!(
        decoded.payload.guidance,
        vec!["use `rg` for exact token verification".to_owned()]
    );
}

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
    assert!(status_payload.stale_paths.is_empty());

    let files_payload = query_files(temp.path(), None).expect("files");
    assert!(!files_payload.freshness.stale);
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

    let search_payload = query_search(temp.path(), "release", Some(10)).expect("search");
    assert!(search_payload.freshness.stale);
    assert!(search_payload
        .freshness
        .stale_paths
        .contains(&"src/lib.rs".to_owned()));
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

#[test]
fn graph_manifest_indexer_emits_effigy_domain_relations() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("scripts")).expect("mkdir scripts");
    fs::create_dir_all(temp.path().join("bundles/workspace-app")).expect("mkdir bundle");
    fs::write(
        temp.path().join("bundles/workspace-app/bundle.toml"),
        r#"
[bundle]
name = "workspace-app"
description = "Workspace app bundle."
"#,
    )
    .expect("write bundle descriptor");
    fs::write(temp.path().join("bundles/workspace-app/effigy.toml"), "")
        .expect("write bundle defaults manifest");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[manifest]
include = ["overlay.toml"]

[tasks.release]
run = [{ task = "build:api" }, { rhai = "scripts/release.rhai" }]
system = "dev"
workspace = "api"

[bundle]
base = { type = "path", dir = "bundles/workspace-app" }

[docs_policy.indexes.vision]
file = "docs/README.md"
dir = "docs"

[docs_policy.next_actions.vision]
index = "vision"
heading = "Next"
allowlist_file = "docs/allowlist.txt"

[test]
max_parallel = 4
cargo_env_match = "prefix-aware"

[test.runners.nextest]
command = "cargo nextest run"

[test.suites.qa]
run = "cargo test"
setup = [{ task = "db:up" }]
teardown = [{ rhai = "scripts/cleanup.rhai" }]

[secrets]
backend = "effigy-vault"

[secrets.vault]
path = ".effigy/secrets.vault"
identity = "ssh-agent"
unlock = "external"

[secrets.keys.deploy_token]
required = true
targets = ["deploy", "tasks"]

[state.uat]
schema = "effigy.state-stack.v1"
environment = "uat"

[[state.uat.layers]]
key = "baseline"
role = "baseline-seed"
source = "db:seed"
apply_mode = "task"
environment_policy = "all"
hook = { rhai = "state/apply-baseline.rhai", run_in = "host" }

[state.uat.captures.media]
role = "full-capture"
source_env = "legacy"
source = ".effigy/state/captures/media"
ref = "oci://ghcr.io/acme/media:{key}"
task = { rhai = "state/capture-media.rhai", run_in = "host" }
"#,
    )
    .expect("write root manifest");
    fs::write(
        temp.path().join("overlay.toml"),
        r#"
[systems.dev]
default_workspace = "api"

[systems.dev.workspaces.api]
container = "app"

[containers.app]
primary-service = "web"

[containers.app.services.web]
catalog = "php-fpm"
variant = "laravel"
config = "configs/web.conf"

[distribution.preflight]
docs-task = "docs:check"
smoke-task = "test:smoke"

[deploy.providers.render]
source = { type = "path", dir = "../external/providers/render" }

[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "render"
project_id = "render-project"
environment_id = "render-env"
services = { front = "front-service" }
"#,
    )
    .expect("write overlay manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "overlay.toml"));

    let search = query_search(temp.path(), "release", Some(20)).expect("search");
    let task = search
        .matches
        .iter()
        .find(|entry| entry.name.as_deref() == Some("task::release"))
        .expect("release task");
    let task_node = node(temp.path(), &task.record_id).expect("task node");
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-step-task" && edge.unresolved_target.as_deref() == Some("build:api")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-step-rhai"
            && edge.unresolved_target.as_deref() == Some("scripts/release.rhai")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-system" && edge.unresolved_target.as_deref() == Some("dev")
    }));
    assert!(task_node.edges.iter().any(|edge| {
        edge.kind == "task-workspace" && edge.unresolved_target.as_deref() == Some("api")
    }));

    let store = GraphStore::open(temp.path()).expect("store");
    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "includes-manifest"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:overlay.toml")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "bundle-base-path"
            && edge.unresolved_target.as_deref() == Some("bundles/workspace-app")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "workspace-container-ref" && edge.unresolved_target.as_deref() == Some("app")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "service-catalog" && edge.unresolved_target.as_deref() == Some("php-fpm")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "distribution-docs-task"
            && edge.unresolved_target.as_deref() == Some("docs:check")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "docs-policy-file"
            && edge.unresolved_target.as_deref() == Some("docs/README.md")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "test-runner-command"
            && edge.unresolved_target.as_deref() == Some("cargo nextest run")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "secret-key-target" && edge.unresolved_target.as_deref() == Some("deploy")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "state-layer-source" && edge.unresolved_target.as_deref() == Some("db:seed")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "state-capture-ref"
            && edge.unresolved_target.as_deref() == Some("oci://ghcr.io/acme/media:{key}")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "deploy-provider-source-path"
            && edge.unresolved_target.as_deref() == Some("../external/providers/render")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "deploy-target-provider"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str().contains("deploy:provider:render"))
    }));
}

#[test]
fn graph_manifest_indexer_emits_bootstrap_task_selector_entrypoints() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[tasks.dev]
run = "cargo run"

[bootstrap]
start = "dev"
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "task-selector" && symbol.canonical_name == "selector::dev"
    }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "entrypoint-task"
            && edge.from_id.as_str().contains("bootstrap:start:dev")
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str().contains("task:dev"))
    }));
}

#[test]
fn graph_manifest_semantic_failures_fall_back_to_structural_indexing() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[tasks.release]
run = "cargo test"

[bundle]
base = { type = "path", dir = "bundles/missing-bundle" }
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert_eq!(report.counts.diagnostics, 1);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "effigy.toml"));

    let search = query_search(temp.path(), "release", Some(10)).expect("search");
    assert!(search
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("task::release")));

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("failed to compose manifest effigy.toml")
    }));
}

#[test]
fn graph_manifest_indexer_structurally_indexes_template_rich_toml_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = fs::read_to_string(format!(
        "{}/../effigy-manifest/tests/fixtures/workspace-app-bundle/export.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read export fixture");
    fs::write(temp.path().join("export.toml"), fixture).expect("write export fixture");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "export.toml"));
}

#[test]
fn graph_manifest_indexer_structurally_indexes_templates_with_embedded_quotes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let fixture = fs::read_to_string(format!(
        "{}/../effigy-manifest/tests/fixtures/php-app-bundle/export.toml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("read export fixture");
    fs::write(temp.path().join("export.toml"), fixture).expect("write export fixture");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "export.toml"));
}

#[test]
fn graph_manifest_indexer_skips_blank_unresolved_targets() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[manifest]
root = true

[deploy.production]
state = "production"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "smoke"
artifact_policy = "digest-preferred"

[deploy.production.provider]
adapter = "render"
project_id = ""
environment_id = ""
services = { front = "" }
"#,
    )
    .expect("write manifest");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics.iter().all(|diagnostic| {
        !diagnostic
            .message
            .contains("edge unresolved target must not be empty")
    }));
}

#[test]
fn graph_markdown_indexer_emits_code_fences_and_local_path_refs() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn render_docs() {}\n").expect("write rust");
    fs::write(
        temp.path().join("docs/guide.md"),
        r#"# Guide

See `../src/lib.rs` and [the source](../src/lib.rs).

```rust
pub fn render_docs() {}
```
"#,
    )
    .expect("write markdown");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "code-fence" && symbol.canonical_name == "docs/guide.md::code-fence::1"
    }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "code-fence-language" && edge.unresolved_target.as_deref() == Some("rust")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "doc-path-ref"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:src/lib.rs")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "doc-link-file"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:src/lib.rs")
    }));
}

#[test]
fn graph_php_indexer_emits_namespace_symbols_and_static_include_edges() {
    let temp = tempfile::tempdir().expect("tempdir");
    write_php_front_controller_fixture(temp.path());

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files
        .files
        .iter()
        .any(|file| file.path == "legacy/index.php"));

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "front-controller" && symbol.canonical_name == "legacy/index.php"
    }));
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "namespace" && symbol.canonical_name == "App\\Controller"
    }));
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "class" && symbol.canonical_name == "App\\Controller\\HomeController"
    }));
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "method"
            && symbol.canonical_name == "App\\Controller\\HomeController::handle"
    }));
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "constant"
            && symbol.canonical_name == "App\\Controller\\HomeController::VERSION"
    }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "include-file"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:legacy/boot.php")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "import"
            && edge.unresolved_target.as_deref() == Some("Legacy\\Support\\Helper")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "call"
            && edge.unresolved_target.as_deref() == Some("App\\Controller\\HomeController::handle")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "call" && edge.unresolved_target.as_deref() == Some("$this->helperName")
    }));
}

#[test]
fn graph_deferred_parity_fixture_cases_are_runnable() {
    struct FixtureCase<'a> {
        id: &'a str,
        query: &'a str,
        expected_primary: &'a str,
        acceptable_primary: &'a [&'a str],
        setup: fn(&Path),
    }

    let cases = [
        FixtureCase {
            id: "affected-test-proxy",
            query: "graph watch regression tests",
            expected_primary: "tests/graph_watch_tests.rs",
            acceptable_primary: &["src/graph/watch.rs"],
            setup: write_graph_watch_fixture,
        },
        FixtureCase {
            id: "cross-language-php-front-controller",
            query: "trace php front controller boot helper",
            expected_primary: "legacy/index.php",
            acceptable_primary: &["legacy/boot.php", "legacy/App/Controller.php"],
            setup: write_php_front_controller_fixture,
        },
    ];

    for case in cases {
        let temp = tempfile::tempdir().expect("tempdir");
        (case.setup)(temp.path());
        run_index(temp.path()).expect("index");

        let payload = explore(temp.path(), case.query, Some(6), Some(12288), &[], &[])
            .expect("fixture explore");
        let primary_paths = payload
            .primary
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>();
        let top_primary = primary_paths
            .first()
            .copied()
            .expect("fixture case should return at least one primary file");

        println!("fixture parity {} -> {}", case.id, top_primary);

        assert!(
            top_primary == case.expected_primary || case.acceptable_primary.contains(&top_primary),
            "fixture case {} returned unexpected primary {} from {:?}",
            case.id,
            top_primary,
            primary_paths
        );
        assert!(
            !payload.excerpts.is_empty(),
            "fixture case {} should emit excerpts for targeted follow-up",
            case.id
        );
    }
}

#[test]
fn graph_php_indexer_emits_parse_diagnostics_without_failing_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("broken.php"),
        "<?php\nfunction broken( {\n",
    )
    .expect("write broken php");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert!(report.counts.diagnostics > 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "broken.php"));

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("php parse error")));
}

#[test]
fn graph_javascript_indexer_emits_import_export_and_component_facts() {
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

export interface ButtonProps {
    label: string;
}

export const Button = ({ label }: ButtonProps) => <button>{label} {helper()}</button>;

export default Button;
"#,
    )
    .expect("write component");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files
        .files
        .iter()
        .any(|file| file.path == "web/components/Button.tsx"));

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols
        .iter()
        .any(|symbol| { symbol.kind == "react-component" && symbol.canonical_name == "Button" }));
    assert!(symbols
        .iter()
        .any(|symbol| { symbol.kind == "interface" && symbol.canonical_name == "ButtonProps" }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "import-file"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:web/util.ts")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "import" && edge.unresolved_target.as_deref() == Some("react")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "export" && edge.unresolved_target.as_deref() == Some("Button")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "export-default" && edge.unresolved_target.as_deref() == Some("default")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "call" && edge.unresolved_target.as_deref() == Some("helper")
    }));
}

#[test]
fn graph_javascript_indexer_emits_parse_diagnostics_without_failing_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("broken.ts"),
        "export const broken = ( => 1;\n",
    )
    .expect("write broken ts");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert!(report.counts.diagnostics > 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "broken.ts"));

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("js/ts parse error")));
}

#[test]
fn graph_python_indexer_emits_import_call_and_class_facts() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("app")).expect("mkdir app");
    fs::write(
        temp.path().join("app/helpers.py"),
        r#"
def slugify(name):
    return name.lower()
"#,
    )
    .expect("write helpers");
    fs::write(
        temp.path().join("app/service.py"),
        r#"
from .helpers import slugify

class UserService:
    def normalize(self, name):
        return slugify(name)
"#,
    )
    .expect("write service");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "app/service.py"));

    let store = GraphStore::open(temp.path()).expect("store");
    let extractors = store.list_extractors().expect("extractors");
    assert!(
        extractors
            .iter()
            .any(|extractor| extractor.id.as_str() == "python-syntax"),
        "python extractor should be registered: {extractors:?}"
    );

    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols
        .iter()
        .any(|symbol| symbol.kind == "class" && symbol.canonical_name == "UserService"));
    assert!(symbols.iter().any(|symbol| {
        symbol.kind == "function" && symbol.canonical_name == "UserService::normalize"
    }));
    assert!(symbols
        .iter()
        .any(|symbol| symbol.kind == "function" && symbol.canonical_name == "slugify"));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "import-file"
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str() == "file:app/helpers.py")
    }));
    assert!(edges.iter().any(|edge| {
        edge.kind == "call" && edge.unresolved_target.as_deref() == Some("slugify")
    }));
}

#[test]
fn graph_python_indexer_emits_parse_diagnostics_without_failing_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("broken.py"),
        "def broken(:\n    return 1\n",
    )
    .expect("write broken python");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);
    assert!(report.counts.diagnostics > 0);

    let files = query_files(temp.path(), None).expect("files");
    assert!(files.files.iter().any(|file| file.path == "broken.py"));

    let store = GraphStore::open(temp.path()).expect("store");
    let diagnostics = store.list_diagnostics().expect("diagnostics");
    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("python parse error")));
}

#[test]
fn graph_python_indexer_emits_route_handler_edges_and_route_queries_find_owner() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("app")).expect("mkdir app");
    fs::write(
        temp.path().join("app/api.py"),
        r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
def list_users():
    return []
"#,
    )
    .expect("write api");

    let report = run_index(temp.path()).expect("index");
    assert_eq!(report.failed_paths.len(), 0);

    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    assert!(symbols
        .iter()
        .any(|symbol| { symbol.kind == "http-route" && symbol.canonical_name == "GET /users" }));

    let edges = store.list_edges().expect("edges");
    assert!(edges.iter().any(|edge| {
        edge.kind == "route-handler"
            && edge.from_id.as_str().contains("/users")
            && edge
                .to_id
                .as_ref()
                .is_some_and(|id| id.as_str().contains("list_users"))
    }));

    let payload = context(
        temp.path(),
        "where is /users handled",
        Some(3),
        Some(4096),
        &["python".to_owned()],
        &[],
    )
    .expect("context");

    assert_eq!(
        payload.items.first().map(|item| item.path.as_str()),
        Some("app/api.py"),
        "route query should find the owning Python file first: {:?}",
        payload
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .items
            .iter()
            .any(|item| { item.kind == "symbol" && item.name.as_deref() == Some("GET /users") }),
        "route query should surface the route symbol: {:?}",
        payload
            .items
            .iter()
            .map(|item| format!("{}::{:?}", item.kind, item.name))
            .collect::<Vec<_>>()
    );
}

#[test]
fn graph_explore_labels_python_sections_and_deduplicates_same_path_excerpts() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("app")).expect("mkdir app");
    fs::write(
        temp.path().join("app/api.py"),
        r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
def list_users():
    return []
"#,
    )
    .expect("write api");

    run_index(temp.path()).expect("index");
    let payload = explore(
        temp.path(),
        "where is /users handled",
        Some(3),
        Some(4096),
        &["python".to_owned()],
        &[],
    )
    .expect("explore");

    let api_excerpts = payload
        .excerpts
        .iter()
        .filter(|item| item.path == "app/api.py")
        .collect::<Vec<_>>();
    assert_eq!(
        api_excerpts.len(),
        1,
        "explore should not repeat the same file excerpt multiple times: {:?}",
        payload
            .excerpts
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(api_excerpts[0].section_kind, "python-block");
    assert_eq!(api_excerpts[0].completeness, "complete-section");
    assert!(api_excerpts[0].text.contains("@app.get(\"/users\")"));
    assert!(api_excerpts[0].text.contains("def list_users():"));
}

#[test]
fn graph_affected_returns_likely_test_files_and_tasks_for_changed_source() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::create_dir_all(temp.path().join("tests")).expect("mkdir tests");
    fs::write(
        temp.path().join("effigy.toml"),
        r#"
[tasks.test]
run = "cargo test"
"#,
    )
    .expect("write manifest");
    fs::write(
        temp.path().join("src/lib.rs"),
        r#"
pub fn helper() -> i32 {
    1
}
"#,
    )
    .expect("write lib");
    fs::write(
        temp.path().join("tests/helper_test.rs"),
        r#"
use demo::helper;

#[test]
fn helper_works() {
    assert_eq!(helper(), 1);
}
"#,
    )
    .expect("write tests");

    run_index(temp.path()).expect("index");
    let payload = affected(temp.path(), &["src/lib.rs".to_owned()], 2, Some(20)).expect("affected");

    assert!(
        payload
            .affected_files
            .iter()
            .any(|item| item.path == "src/lib.rs"),
        "changed file should be present in affected files: {:?}",
        payload
            .affected_files
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .likely_test_files
            .iter()
            .any(|item| item.path == "tests/helper_test.rs"),
        "test file should be discovered from graph adjacency: {:?}",
        payload
            .likely_test_files
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        payload
            .likely_test_tasks
            .iter()
            .any(|item| item.name == "test"),
        "manifest test task should be surfaced as a candidate: {:?}",
        payload
            .likely_test_tasks
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>()
    );
}
