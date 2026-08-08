use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{
    parse_json_output_with_schema_version, temp_workspace, write_root_manifest,
};
use effigy_cli::{Command, GraphArgs, GraphSubcommand};
use std::fs;

fn setup_graph_fixture(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::create_dir_all(root.join("tests")).expect("mkdir tests");
    fs::create_dir_all(root.join("web")).expect("mkdir web");

    write_root_manifest(
        &root,
        r#"
[tasks.release]
run = "cargo test"

[tasks.test]
run = "cargo test"
"#,
    );
    fs::write(
        root.join("src/lib.rs"),
        r#"
pub fn release_graph() {
    helper();
}

fn helper() {}
"#,
    )
    .expect("write rust");
    fs::write(
        root.join("docs/README.md"),
        "# Release Graph\n\nSee [manifest](../effigy.toml).\n",
    )
    .expect("write docs");
    fs::write(
        root.join("tests/release_graph_test.rs"),
        r#"
use demo::release_graph;

#[test]
fn release_graph_runs() {
    release_graph();
}
"#,
    )
    .expect("write tests");
    fs::write(
        root.join("web/index.ts"),
        "export function renderRelease() { return helper(); }\nfunction helper() { return 1; }\n",
    )
    .expect("write ts");
    root
}

#[test]
fn graph_index_and_status_json_report_repo_state() {
    let root = setup_graph_fixture("graph-index-status-json");

    let indexed = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph index should succeed");
    let indexed = parse_json_output_with_schema_version(&indexed, "effigy.graph.index.v1", 1);
    assert_eq!(indexed["command"].as_str(), Some("graph index"));
    assert!(indexed["payload"]["indexed_files"].as_u64().unwrap_or(0) >= 4);
    assert!(indexed["payload"]["counts"]["files"].as_u64().unwrap_or(0) >= 4);
    assert_eq!(
        indexed["payload"]["failed_paths"]
            .as_array()
            .expect("failed paths")
            .len(),
        0
    );

    let status = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: false },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph status should succeed");
    let status = parse_json_output_with_schema_version(&status, "effigy.graph.status.v1", 1);
    assert_eq!(status["command"].as_str(), Some("graph status"));
    assert_eq!(status["payload"]["ready"].as_bool(), Some(true));
    assert_eq!(
        status["payload"]["freshness"]["state"].as_str(),
        Some("ready")
    );
    assert_eq!(
        status["payload"]["freshness"]["usable"].as_bool(),
        Some(true)
    );
    assert_eq!(
        status["payload"]["stale_paths"]
            .as_array()
            .expect("stale paths")
            .len(),
        0
    );
}

#[test]
fn graph_search_and_context_json_return_ranked_results() {
    let root = setup_graph_fixture("graph-search-context-json");
    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph index should succeed");

    let search = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Search {
            query: "release".to_owned(),
            limit: Some(10),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph search should succeed");
    let search = parse_json_output_with_schema_version(&search, "effigy.graph.search.v1", 1);
    assert_eq!(
        search["payload"]["freshness"]["stale"].as_bool(),
        Some(false)
    );
    assert_eq!(
        search["payload"]["freshness"]["state"].as_str(),
        Some("ready")
    );
    assert!(!search["payload"]["matches"]
        .as_array()
        .expect("matches")
        .is_empty());

    let context = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Context {
            request: "trace release helper".to_owned(),
            max_files: Some(4),
            max_bytes: Some(4096),
            languages: vec!["rust".to_owned(), "markdown".to_owned()],
            paths: vec![],
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph context should succeed");
    let context = parse_json_output_with_schema_version(&context, "effigy.graph.context.v1", 1);
    assert_eq!(
        context["payload"]["freshness"]["stale"].as_bool(),
        Some(false)
    );
    assert_eq!(
        context["payload"]["freshness"]["state"].as_str(),
        Some("ready")
    );
    assert!(!context["payload"]["items"]
        .as_array()
        .expect("items")
        .is_empty());
    assert!(context["payload"]["items"]
        .as_array()
        .expect("items")
        .iter()
        .all(|item| item["reasons"]
            .as_array()
            .is_some_and(|reasons| !reasons.is_empty())));
    assert!(
        context["payload"]["overflow"]["byte_budget"]
            .as_u64()
            .unwrap_or(0)
            >= context["payload"]["overflow"]["used_bytes"]
                .as_u64()
                .unwrap_or(u64::MAX)
    );
    assert!(context["payload"]["notes"]
        .as_array()
        .expect("notes")
        .iter()
        .any(|value| value.as_str() == Some("language filter: rust,markdown")));

    let explore = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Explore {
            request: "trace release helper".to_owned(),
            max_files: Some(4),
            max_bytes: Some(8192),
            languages: vec!["rust".to_owned(), "markdown".to_owned()],
            paths: vec![],
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph explore should succeed");
    let explore = parse_json_output_with_schema_version(&explore, "effigy.graph.explore.v1", 1);
    assert_eq!(explore["command"].as_str(), Some("graph explore"));
    assert_eq!(
        explore["payload"]["index"]["freshness"]["stale"].as_bool(),
        Some(false)
    );
    assert_eq!(
        explore["payload"]["index"]["freshness"]["state"].as_str(),
        Some("ready")
    );
    assert!(!explore["payload"]["primary"]
        .as_array()
        .expect("primary")
        .is_empty());
    assert!(!explore["payload"]["excerpts"]
        .as_array()
        .expect("excerpts")
        .is_empty());
    assert!(!explore["payload"]["edit_targets"]
        .as_array()
        .expect("edit targets")
        .is_empty());
    assert!(explore["payload"]["likely_test_files"]
        .as_array()
        .expect("likely test files")
        .iter()
        .any(|item| item["path"].as_str() == Some("tests/release_graph_test.rs")));
    assert!(explore["payload"]["likely_test_tasks"]
        .as_array()
        .expect("likely test tasks")
        .iter()
        .any(|item| item["name"].as_str() == Some("test")));
    assert!(explore["payload"]["guidance"]
        .as_array()
        .expect("guidance")
        .iter()
        .any(|value| value.as_str().is_some_and(|text| text.contains("rg"))));
}

#[test]
fn graph_text_commands_render_useful_summaries() {
    let root = setup_graph_fixture("graph-text-commands");
    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph index should succeed");

    let search = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Search {
            query: "release".to_owned(),
            limit: Some(10),
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph search should succeed");
    assert!(search.contains("graph search `release`"));
    assert!(search.contains("symbol"));

    let files = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Files { limit: Some(10) },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph files should succeed");
    assert!(files.contains("graph files:"));
    assert!(files.contains("src/lib.rs"));

    let context = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Context {
            request: "trace release helper".to_owned(),
            max_files: Some(4),
            max_bytes: Some(4096),
            languages: vec![],
            paths: vec![],
        },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph context should succeed");
    assert!(context.contains("graph context `trace release helper`"));
    assert!(context.contains("rank 1"));
    assert!(context.contains("because"));

    let explore = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Explore {
            request: "trace release helper".to_owned(),
            max_files: Some(4),
            max_bytes: Some(8192),
            languages: vec![],
            paths: vec![],
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("graph explore should succeed");
    assert!(explore.contains("graph explore `trace release helper`"));
    assert!(explore.contains("primary:"));
    assert!(explore.contains("edit-targets:"));
    assert!(explore.contains("likely test file"));
    assert!(explore.contains("guidance"));
}

#[test]
fn graph_affected_json_and_text_report_likely_validation_targets() {
    let root = setup_graph_fixture("graph-affected");
    fs::create_dir_all(root.join("tests")).expect("mkdir tests");
    fs::write(
        root.join("tests/release_graph_test.rs"),
        r#"
use demo::release_graph;

#[test]
fn release_graph_smoke() {
    release_graph();
}
"#,
    )
    .expect("write test file");

    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph index should succeed");

    let json = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Affected {
            changed_paths: vec!["src/lib.rs".to_owned()],
            read_stdin: false,
            depth: 2,
            limit: Some(20),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph affected should succeed");
    let json = parse_json_output_with_schema_version(&json, "effigy.graph.affected.v1", 1);
    assert_eq!(json["command"].as_str(), Some("graph affected"));
    assert!(json["payload"]["likely_test_files"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
    assert!(json["payload"]["likely_test_tasks"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));

    let text = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Affected {
            changed_paths: vec!["src/lib.rs".to_owned()],
            read_stdin: false,
            depth: 2,
            limit: Some(20),
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("graph affected text should succeed");
    assert!(text.contains("graph affected:"));
    assert!(text.contains("test-file"));
    assert!(text.contains("test-task"));
}

#[test]
fn graph_query_text_auto_refreshes_stale_index() {
    let root = setup_graph_fixture("graph-text-auto-refresh");
    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph index should succeed");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    // Status still reports the stale index...
    let status = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: false },
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph status should succeed");
    assert!(status.contains("trust: refresh-recommended"));

    // ...while queries refresh on the fly instead of returning stale data.
    let search = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Search {
            query: "release".to_owned(),
            limit: Some(10),
        },
        repo_override: Some(root),
        output_json: false,
    }))
    .expect("graph search should succeed");
    assert!(search.contains("graph trust: ready"));
    assert!(search.contains("graph auto-refreshed"));
    assert!(!search.contains("paths require reindex"));
}

#[test]
fn graph_status_json_reports_missing_index_trust_state() {
    let root = setup_graph_fixture("graph-status-missing-index");

    let status = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: false },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("graph status should succeed");
    let status = parse_json_output_with_schema_version(&status, "effigy.graph.status.v1", 1);
    assert_eq!(status["payload"]["ready"].as_bool(), Some(false));
    assert_eq!(
        status["payload"]["freshness"]["state"].as_str(),
        Some("missing-index")
    );
    assert_eq!(
        status["payload"]["freshness"]["usable"].as_bool(),
        Some(false)
    );
}

#[test]
fn graph_status_refresh_flag_remediates_stale_index() {
    let root = setup_graph_fixture("graph-status-refresh");
    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.clone()),
        output_json: false,
    }))
    .expect("graph index should succeed");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    // Plain status reports the stale index.
    let plain = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: false },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect("graph status should succeed");
    let plain = parse_json_output_with_schema_version(&plain, "effigy.graph.status.v1", 1);
    assert_eq!(
        plain["payload"]["freshness"]["state"].as_str(),
        Some("refresh-recommended")
    );

    // `status --refresh` rebuilds on demand and reports the fresh state.
    let refreshed = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh: true },
        repo_override: Some(root),
        output_json: true,
    }))
    .expect("graph status --refresh should succeed");
    let refreshed = parse_json_output_with_schema_version(&refreshed, "effigy.graph.status.v1", 1);
    assert_eq!(
        refreshed["payload"]["freshness"]["state"].as_str(),
        Some("ready")
    );
    assert_eq!(
        refreshed["payload"]["freshness"]["usable"].as_bool(),
        Some(true)
    );
    assert!(
        refreshed["payload"]["freshness"]["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("graph auto-refreshed")),
        "refresh note should surface in the summary"
    );
}
