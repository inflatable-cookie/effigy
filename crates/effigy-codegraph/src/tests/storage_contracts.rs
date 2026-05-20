use super::*;

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
                used_bytes: 18,
            },
            guidance: vec!["Run `rg watch_repo src` to inspect callers.".to_owned()],
        },
    );
    let json = serde_json::to_string(&payload).expect("serialize");
    let decoded: GraphCommandPayload<GraphExplorePayload> =
        serde_json::from_str(&json).expect("deserialize");
    assert_eq!(decoded.schema, "effigy.graph.explore.v1");
    assert_eq!(decoded.schema_version, GRAPH_JSON_SCHEMA_VERSION);
    assert_eq!(decoded.payload.query, "trace graph watch implementation");
}
