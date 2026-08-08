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

mod context_quality;
mod git_gate;
mod index_lifecycle;
mod language_indexers;
mod manifest_semantics;
mod refresh_lazy;
mod storage_contracts;

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
