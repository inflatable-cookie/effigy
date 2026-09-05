use crate::runner::entrypoints::run_command;
use crate::runner::tests::prelude::{temp_workspace, EnvGuard};
use effigy_cli::{Command, DocsArgs, DocsSubcommand, GraphArgs, GraphSubcommand};
use std::fs;
use std::time::{Duration, Instant};

const TINY_BOUND_MS: &str = "1";
const DISABLED_BOUND_MS: &str = "0";
const TIMEOUT_SCHEMA: &str = "effigy.graph.timeout.v1";

fn docs_graph_fixture(name: &str) -> std::path::PathBuf {
    let root = temp_workspace(name);
    fs::create_dir_all(root.join("docs")).expect("mkdir docs");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    fs::write(
        root.join("docs/README.md"),
        "# Docs Home\n\nThe contracts define the working rules.\n",
    )
    .expect("write docs");
    for index in 0..12 {
        fs::write(
            root.join(format!("docs/topic-{index}.md")),
            format!("# Topic {index}\n\nThe contracts govern topic {index} and its roadmap.\n"),
        )
        .expect("write topic doc");
    }
    fs::write(root.join("src/lib.rs"), "pub fn release_helper() {}\n").expect("write rust");
    root
}

fn docs_context_command(root: &std::path::Path, query: &str, output_json: bool) -> Command {
    Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::Context {
            query: query.to_owned(),
            max_sections: None,
            max_bytes: None,
            max_hops: None,
        },
        repo_override: Some(root.to_path_buf()),
        output_json,
    })
}

fn assert_typed_graph_timeout(
    error: crate::runner::error::RunnerError,
    command: &str,
    bound_ms: u64,
) {
    let rendered = error
        .rendered_output()
        .expect("timeout must carry rendered detail");
    let parsed: serde_json::Value = serde_json::from_str(rendered).expect("valid json detail");
    assert_eq!(parsed["schema"], TIMEOUT_SCHEMA);
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["command"], command);
    assert_eq!(parsed["timeout_ms"], bound_ms);
    assert_eq!(parsed["timeout_env"], "EFFIGY_GRAPH_TIMEOUT_MS");
    assert!(
        parsed["health"].is_object(),
        "timeout detail must carry the shared health snapshot"
    );
    let next = parsed["next"].as_array().expect("recovery guidance");
    assert!(
        next.iter().any(|step| step
            .as_str()
            .is_some_and(|text| text.contains("graph status"))),
        "recovery must share graph-status guidance: {next:?}"
    );
}

#[test]
fn docs_cold_refresh_fails_within_a_deliberately_tiny_bound() {
    let root = docs_graph_fixture("docs-cold-tiny-bound");
    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);
    let started = Instant::now();
    let error = run_command(docs_context_command(&root, "contracts", false))
        .expect_err("cold refresh must fail inside the tiny bound");
    let elapsed = started.elapsed();

    assert_typed_graph_timeout(error, "docs context", 1);
    assert!(
        elapsed < Duration::from_secs(5),
        "cold docs refresh exceeded the bound: {elapsed:?}"
    );
}

#[test]
fn docs_stale_refresh_fails_within_a_deliberately_tiny_bound() {
    let root = docs_graph_fixture("docs-stale-tiny-bound");
    run_graph_index(&root);
    fs::write(
        root.join("docs/topic-3.md"),
        "# Topic 3\n\nThe contracts govern topic 3 and its roadmap, refreshed.\n",
    )
    .expect("stale the index");

    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);
    let started = Instant::now();
    let error = run_command(docs_context_command(&root, "contracts", false))
        .expect_err("stale refresh must fail inside the tiny bound");
    let elapsed = started.elapsed();

    assert_typed_graph_timeout(error, "docs context", 1);
    assert!(
        elapsed < Duration::from_secs(5),
        "stale docs refresh exceeded the bound: {elapsed:?}"
    );
}

#[test]
fn docs_cold_and_warm_queries_succeed_when_the_bound_is_disabled() {
    let root = docs_graph_fixture("docs-bound-disabled");
    let _env = EnvGuard::set_many(&[(
        "EFFIGY_GRAPH_TIMEOUT_MS",
        Some(DISABLED_BOUND_MS.to_owned()),
    )]);

    let cold = run_command(docs_context_command(&root, "contracts", false))
        .expect("bound 0 must not disable the query, only the timeout");
    assert!(cold.contains("docs context"), "text output: {cold}");

    let warm = run_command(docs_context_command(&root, "contracts", true))
        .expect("warm query with a disabled bound must succeed");
    let parsed: serde_json::Value =
        serde_json::from_str(&warm).expect("JSON stdout must stay valid");
    assert_eq!(parsed["schema"], "effigy.docs.context.v1");
    assert_eq!(parsed["schema_version"], 1);
}

#[test]
fn empty_query_usage_error_wins_over_a_tiny_bound() {
    let root = docs_graph_fixture("docs-empty-query-tiny-bound");
    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);

    let error = run_command(docs_context_command(&root, "   ", false))
        .expect_err("empty query is a usage error");
    assert!(
        error.to_string().contains("non-empty query"),
        "usage error must surface: {error}"
    );
    assert_eq!(
        error.rendered_output(),
        None,
        "usage errors must not be rewrapped as graph timeouts"
    );
}

#[test]
fn invalid_budget_usage_errors_win_over_a_tiny_bound() {
    let root = docs_graph_fixture("docs-invalid-budget-tiny-bound");
    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);

    let zero = docs_context_command(&root, "contracts", false);
    let Command::Docs(mut args) = zero else {
        panic!("expected docs command");
    };
    let DocsSubcommand::Context { max_sections, .. } = &mut args.subcommand else {
        panic!("expected docs context subcommand");
    };
    *max_sections = Some(0);
    let error = run_command(Command::Docs(args)).expect_err("zero budget is a usage error");
    assert!(
        error.to_string().contains("must be greater than 0"),
        "usage error must surface: {error}"
    );

    let over_max = docs_context_command(&root, "contracts", false);
    let Command::Docs(mut args) = over_max else {
        panic!("expected docs command");
    };
    let DocsSubcommand::Context { max_sections, .. } = &mut args.subcommand else {
        panic!("expected docs context subcommand");
    };
    *max_sections = Some(999);
    let error = run_command(Command::Docs(args)).expect_err("over-max budget is a usage error");
    assert!(
        error.to_string().contains("must be at most 32"),
        "usage error must surface: {error}"
    );
}

#[test]
fn timeout_detail_names_the_phase_the_bound_expired_in() {
    let root = docs_graph_fixture("docs-timeout-phase-detail");
    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);

    let error = run_command(docs_context_command(&root, "contracts", false))
        .expect_err("cold refresh must fail inside the tiny bound");
    let rendered = error
        .rendered_output()
        .expect("timeout must carry rendered detail")
        .to_owned();
    let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("valid json detail");

    // The phase block is additive: the frozen schema identity and every
    // pre-existing field stay exactly as they were.
    assert_eq!(parsed["schema"], TIMEOUT_SCHEMA);
    assert_eq!(parsed["schema_version"], 1);
    assert!(parsed["health"].is_object());

    let next = parsed["next"].as_array().expect("recovery guidance");
    assert!(
        next.iter().any(|step| step
            .as_str()
            .is_some_and(|text| text.contains("graph status"))),
        "pre-existing recovery guidance must survive: {next:?}"
    );

    // The bound is deliberately tiny, so the worker may not have reached graph
    // work at all. Both shapes are contractual: a phase block naming a known
    // phase, or JSON null. What is never allowed is a name outside the closed
    // set, half-reported progress, or a phase carried over from earlier work —
    // the bounded runner clears the record before every run.
    match parsed["phase"].as_object() {
        None => assert!(
            parsed["phase"].is_null(),
            "phase must be an object or null: {}",
            parsed["phase"]
        ),
        Some(phase) => {
            let name = phase["name"].as_str().expect("phase name");
            assert!(
                effigy_codegraph::KNOWN_PHASE_NAMES.contains(&name),
                "unknown phase name in timeout detail: {name}"
            );
            assert!(phase["elapsed_ms"].is_u64(), "phase detail: {phase:?}");
            match (phase["items_done"].as_u64(), phase["items_total"].as_u64()) {
                (Some(done), Some(total)) => assert!(done <= total, "{done}/{total}"),
                (None, None) => {}
                other => panic!("progress must report both bounds or neither: {other:?}"),
            }
            assert!(
                next.iter().any(|step| step
                    .as_str()
                    .is_some_and(|text| text.contains(name) && text.contains("bound expired"))),
                "recovery guidance must name the phase in text too: {next:?}"
            );
        }
    }
}

#[test]
fn graph_search_timeout_behavior_is_unchanged() {
    let root = docs_graph_fixture("graph-search-tiny-bound");
    let _env = EnvGuard::set_many(&[("EFFIGY_GRAPH_TIMEOUT_MS", Some(TINY_BOUND_MS.to_owned()))]);
    let error = run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Search {
            query: "release_helper".to_owned(),
            limit: Some(10),
        },
        repo_override: Some(root.clone()),
        output_json: true,
    }))
    .expect_err("cold graph search must stay bounded");

    assert_typed_graph_timeout(error, "graph search", 1);
}

fn run_graph_index(root: &std::path::Path) {
    run_command(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override: Some(root.to_path_buf()),
        output_json: true,
    }))
    .expect("graph index should succeed");
}
