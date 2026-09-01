use super::*;

const LIVE_CASE_FRAGMENTS: &[&str] = &[
    r#"id: "effigy-contract-authority",
                dimension: "contract",
                expect: "live-authority",
                query: "documentation graph profile contract",
                extra_args: [],
                expected_path: "docs/contracts/041-documentation-graph-profile-contract.md",
                rival_path: "docs/logs/2026-08/31-181957-documentation-context-1089.md",
                max_rank: 3,"#,
    r#"id: "effigy-architecture-authority",
                dimension: "architecture",
                expect: "live-authority",
                query: "repository defined documentation graph architecture",
                extra_args: [],
                expected_path: "docs/architecture/024-repository-defined-documentation-graph.md",
                rival_path: "docs/roadmaps/g08/batch-cards/1088-build-documentation-profile-and-structural-index.md",
                max_rank: 3,"#,
    r#"id: "effigy-direct-historical-guide",
                dimension: "historical-decision",
                expect: "historical-retrieval",
                query: "docs consistency sweep and changelog",
                extra_args: [],
                expected_path: "docs/guides/archive/032-docs-consistency-sweep-and-changelog.md",
                max_rank: 8,"#,
    r#"id: "effigy-next-task",
                dimension: "next-task",
                expect: "live-authority",
                query: "active strict lane spec set",
                extra_args: [],
                expected_path: "docs/specs/README.md",
                rival_path: "docs/specs/archive/107-documentation-coverage-parity.md",
                max_rank: 3,"#,
    r#"id: "effigy-historical-decision",
                dimension: "historical-decision",
                expect: "historical-retrieval",
                query: "bounded documentation context query card 1089 closeout evidence",
                extra_args: [],
                expected_path: "docs/logs/2026-08/31-181957-documentation-context-1089.md",
                max_rank: 8,"#,
];

fn benchmark_script_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("scripts/benchmark-docs-context.rhai")
}

fn benchmark_script_source() -> String {
    fs::read_to_string(benchmark_script_path()).expect("read docs-context benchmark script")
}

fn strip_line_comments(source: &str) -> String {
    source
        .lines()
        .map(|line| match line.find("//") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_rhai_fn<'a>(source: &'a str, name: &str) -> &'a str {
    let needle = format!("fn {name}(");
    let start = source
        .find(&needle)
        .unwrap_or_else(|| panic!("missing `{needle}`"));
    let bytes = source.as_bytes();
    let mut brace_at = start;
    while brace_at < bytes.len() && bytes[brace_at] != b'{' {
        brace_at += 1;
    }
    assert!(brace_at < bytes.len(), "missing body for `{name}`");
    let mut depth = 0i32;
    let mut end = brace_at;
    while end < bytes.len() {
        match bytes[end] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[start..=end];
                }
            }
            _ => {}
        }
        end += 1;
    }
    panic!("unclosed `{name}`");
}

fn run_guard(harness: &str) -> Result<(), String> {
    let source = benchmark_script_source();
    let guard = extract_rhai_fn(&source, "reject_live_empty_cases");
    let script = format!("{guard}\n{harness}");
    let root = temp_root("docs-context-empty-guard");
    execute_rhai_script(&script_context(&root), &script, &[], &callbacks())
        .map_err(|error| error.to_string())
}

#[test]
fn docs_context_benchmark_owns_empty_proof_on_the_fixture_only() {
    let source = strip_line_comments(&benchmark_script_source());
    assert!(
        source.contains("id: \"generic-no-match\""),
        "fixture empty proof must remain"
    );
    assert!(
        source.contains("query: \"quokka marmalade trombone\""),
        "fixture empty query must remain non-vacuous"
    );
    assert_eq!(
        source.matches("expect: \"empty\"").count(),
        1,
        "exactly one empty-result case must remain, on the fixture"
    );
    assert!(
        !source.contains("id: \"effigy-no-match\""),
        "live empty case must leave the current matrix"
    );
}

#[test]
fn docs_context_benchmark_preserves_live_authority_and_historical_cases() {
    let source = benchmark_script_source();
    for fragment in LIVE_CASE_FRAGMENTS {
        assert!(source.contains(fragment), "live case drifted:\n{fragment}");
    }
}

#[test]
fn docs_context_benchmark_validates_live_empty_cases_before_queries() {
    let source = benchmark_script_source();
    let call = source
        .find("reject_live_empty_cases(repo_root, targets);")
        .expect("matrix guard must be invoked");
    let index = source
        .find("run_graph_index(bin_path, repo_path);")
        .expect("index step must exist");
    let query = source
        .find("let case_result = evaluate_case(")
        .expect("query step must exist");
    assert!(
        call < index && index < query,
        "live empty cases must fail before indexing or query execution"
    );
}

#[test]
fn docs_context_benchmark_rejects_a_live_target_empty_case_before_query_execution() {
    let error = run_guard(
        r#"
let live = "/live-repo";
let targets = [
    #{
        label: "effigy-live",
        repo: live,
        cases: [
            #{ id: "live-empty", expect: "empty" },
        ],
    },
];
reject_live_empty_cases(live, targets);
throw "reached query execution";
"#,
    )
    .expect_err("live-target empty case must fail matrix validation");
    assert!(
        error.contains("live-empty"),
        "guard must name the rejected case: {error}"
    );
    assert!(
        error.contains("live repository"),
        "guard must name the live-target rule: {error}"
    );
    assert!(
        error.contains("fixture corpus"),
        "guard must name the fixture-only rule: {error}"
    );
    assert!(
        !error.contains("reached query execution"),
        "guard must throw before query execution: {error}"
    );
}

#[test]
fn docs_context_benchmark_allows_fixture_empty_and_live_nonempty_cases() {
    run_guard(
        r#"
let live = "/live-repo";
let fixture = "/fixture-repo";
let targets = [
    #{
        label: "generic-handbook",
        repo: fixture,
        cases: [
            #{ id: "generic-no-match", expect: "empty" },
        ],
    },
    #{
        label: "effigy-live",
        repo: live,
        cases: [
            #{ id: "effigy-contract-authority", expect: "live-authority" },
        ],
    },
];
reject_live_empty_cases(live, targets);
"#,
    )
    .expect("fixture empty proof and live non-empty cases must pass matrix validation");
}
