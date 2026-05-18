use crate::json::{
    render_json, GraphCommandPayload, GraphContextItemPayload, GraphContextOverflowPayload,
    GraphContextPayload, GraphCountsPayload, GraphStatusPayload,
};
use crate::model::{
    Confidence, DiagnosticRecord, DiagnosticSeverity, EdgeRecord, ExtractorCapability,
    ExtractorRecord, FileIndexStatus, FileRecord, IndexRunRecord, Provenance, ReferenceRecord,
    SourcePosition, SourceSpan, SymbolRecord, GRAPH_STORAGE_SCHEMA_VERSION,
};
use crate::{
    callers, context, impact, node, query_files, query_search, run_index, status, CodeGraphError,
    ExtractorId, GraphId, GraphStore, GRAPH_JSON_SCHEMA_VERSION,
};
use std::fs;

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
    fs::create_dir_all(temp.path().join("legacy/App")).expect("mkdir app");
    fs::write(
        temp.path().join("legacy/boot.php"),
        "<?php\nconst BOOTSTRAPPED = true;\n",
    )
    .expect("write boot");
    fs::write(
        temp.path().join("legacy/index.php"),
        r#"<?php
require_once 'boot.php';
App\Controller\HomeController::handle();
"#,
    )
    .expect("write front controller");
    fs::write(
        temp.path().join("legacy/App/Controller.php"),
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
        helper();
    }

    public function render() {
        return $this->helperName();
    }
}

function helper() {
    return true;
}
"#,
    )
    .expect("write php source");

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
