use super::*;

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
