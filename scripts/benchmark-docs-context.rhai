// Documentation-context retrieval benchmark.
//
// The corpus below is PREDECLARED. Queries, expected live authorities,
// historical rivals, and pass criteria are frozen in this file and committed
// before the benchmark is executed, so a green table cannot be manufactured by
// tuning a query after seeing a result. A miss is recorded and diagnosed, not
// edited away.
//
// Freeze history, so a reviewer can replay both states:
//
//   398a698f2  first freeze. 9 of 11 cases held, including every live-authority
//              and historical-retrieval case. Two cases missed for reasons in
//              the case specification, not in the runtime:
//              `effigy-no-match` used a query whose terms are present in this
//              corpus, and `effigy-next-task` asserted a typed-relation hop on a
//              2319-document corpus where lexical seeds always exhaust the
//              section budget before a 1-hop result can be selected. Traversal
//              ranks after every 0-hop result by contract, so it supplements thin
//              lexical evidence rather than competing with it; the typed-relation
//              proof lives in the `generic-handbook` target, where the corpus is
//              small enough for traversal to be observable.
//   this file  second freeze. Three Effigy cases were re-anchored on surfaces
//              that survive their own lane closing, and the no-match query was
//              replaced with terms that are genuinely absent. No ranking rule,
//              weight, or budget default was changed at any point.
//
// Two targets, both required:
//
//   generic-handbook  a non-Northstar fixture whose roots, kinds, fields,
//                     statuses, and relations are arbitrary vocabulary. It
//                     proves the runtime carries no repository-specific
//                     semantics.
//   effigy-live       this repository, using the `[docs_policy.graph]` profile
//                     committed in `docs/effigy.docs.toml`.
//
// Pass criteria, per case `expect`:
//
//   live-authority        the expected current, authoritative source appears at
//                         rank <= max_rank, and the declared historical-only
//                         rival does not rank above it
//   historical-retrieval  a query that directly names historical material still
//                         retrieves that historical source at any rank
//   traversal             the expected source is reached over a configured typed
//                         relation (hops >= 1), and the named relation appears in
//                         at least one result's relation path
//   absent                a high-authority document with no lexical relationship
//                         to the query stays out of the report
//   empty                 a no-match query is a successful empty report, never a
//                         fallback to arbitrary files

let report_dir = path::join(repo_root, ".effigy/perf/docs-context-benchmark");
fs::create_dir(report_dir);

let fixture_root = path::join(repo_root, "tests/fixtures/docs-context-benchmark");

let bin_path = if !args.is_empty() {
    args[0].to_string()
} else if fs::exists(path::join(repo_root, ".local-install/bin/effigy")) {
    path::join(repo_root, ".local-install/bin/effigy")
} else if fs::exists(path::join(repo_root, "target/release/effigy")) {
    path::join(repo_root, "target/release/effigy")
} else if fs::exists(path::join(repo_root, "target/debug/effigy")) {
    path::join(repo_root, "target/debug/effigy")
} else {
    "effigy"
};

let targets = [
    #{
        label: "generic-handbook",
        repo: path::join(fixture_root, "generic-handbook"),
        cases: [
            #{
                id: "generic-charter-authority",
                dimension: "contract",
                expect: "live-authority",
                query: "widget calibrator tolerance band",
                extra_args: [],
                expected_path: "handbook/charters/widget-calibrator.md",
                rival_path: "handbook/bulletins/widget-calibrator-recall.md",
                max_rank: 3,
            },
            #{
                id: "generic-current-over-retired",
                dimension: "current-roadmap",
                expect: "live-authority",
                query: "escalation rota paging order",
                extra_args: [],
                expected_path: "handbook/playbooks/escalation-rota.md",
                rival_path: "handbook/bulletins/escalation-rota-notice.md",
                max_rank: 3,
            },
            #{
                id: "generic-historical-direct",
                dimension: "historical-decision",
                expect: "historical-retrieval",
                query: "recall of 1998",
                extra_args: [],
                expected_path: "handbook/bulletins/widget-calibrator-recall.md",
                max_rank: 8,
            },
            #{
                id: "generic-authority-gate",
                dimension: "authority-gate",
                expect: "absent",
                query: "widget calibrator tolerance band",
                extra_args: [],
                forbidden_path: "handbook/charters/procurement.md",
                max_rank: 8,
            },
            #{
                id: "generic-relation-follow-up",
                dimension: "next-task",
                expect: "traversal",
                query: "widget calibrator tolerance band",
                extra_args: ["--max-hops", "1", "--max-sections", "12"],
                expected_path: "handbook/playbooks/rebalance.md",
                expected_relation: "follow-up",
                max_rank: 12,
            },
        ],
    },
    #{
        label: "effigy-live",
        repo: repo_root,
        cases: [
            #{
                id: "effigy-contract-authority",
                dimension: "contract",
                expect: "live-authority",
                query: "documentation graph profile contract",
                extra_args: [],
                expected_path: "docs/contracts/041-documentation-graph-profile-contract.md",
                rival_path: "docs/logs/2026-08/31-181957-documentation-context-1089.md",
                max_rank: 3,
            },
            #{
                id: "effigy-architecture-authority",
                dimension: "architecture",
                expect: "live-authority",
                query: "repository defined documentation graph architecture",
                extra_args: [],
                expected_path: "docs/architecture/024-repository-defined-documentation-graph.md",
                rival_path: "docs/roadmaps/g08/batch-cards/1088-build-documentation-profile-and-structural-index.md",
                max_rank: 3,
            },
            // Re-anchored in the second freeze. Both sides of this pair are
            // generation front doors with the same kind and the same authority
            // weight; only `Status:` differs, and neither is touched by the lane
            // that ships this benchmark.
            #{
                id: "effigy-current-roadmap",
                dimension: "current-roadmap",
                expect: "live-authority",
                query: "roadmap generation theme purpose",
                extra_args: [],
                expected_path: "docs/roadmaps/g08/README.md",
                rival_path: "docs/roadmaps/g07/README.md",
                max_rank: 3,
            },
            // Re-anchored in the second freeze. The strict-lane front door is the
            // standing answer to "what is the next task", and unlike a ready card
            // it survives its own lane closing.
            #{
                id: "effigy-next-task",
                dimension: "next-task",
                expect: "live-authority",
                query: "active strict lane spec set",
                extra_args: [],
                expected_path: "docs/specs/README.md",
                rival_path: "docs/specs/archive/107-documentation-coverage-parity.md",
                max_rank: 3,
            },
            #{
                id: "effigy-historical-decision",
                dimension: "historical-decision",
                expect: "historical-retrieval",
                query: "bounded documentation context query card 1089 closeout evidence",
                extra_args: [],
                expected_path: "docs/logs/2026-08/31-181957-documentation-context-1089.md",
                max_rank: 8,
            },
            // Re-anchored in the second freeze. The first query was not a
            // no-match query for this corpus: `telemetry` reaches 33 documents
            // and `quokka` reaches 1, so a non-empty report was correct. Both
            // terms below have a corpus document frequency of 0.
            #{
                id: "effigy-no-match",
                dimension: "no-match",
                expect: "empty",
                query: "xylophone sublimation",
                extra_args: [],
                max_rank: 8,
            },
        ],
    },
];

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if result["success"] {
        return result;
    }
    throw "[error] " + label + " failed with status " + result["status"].to_string()
        + "\nstdout:\n" + result["stdout"].to_string()
        + "\nstderr:\n" + result["stderr"].to_string();
}

fn run_graph_index(bin_path, repo_path) {
    log("[info] indexing " + repo_path);
    run_check(
        "graph index " + repo_path,
        bin_path,
        ["graph", "index", "--repo", repo_path, "--json"],
    );
}

fn run_docs_context(bin_path, repo_path, benchmark_case) {
    let process_args = ["--json", "docs", "context", benchmark_case["query"].to_string()];
    for extra in benchmark_case["extra_args"] {
        process_args.push(extra.to_string());
    }
    process_args.push("--repo");
    process_args.push(repo_path.to_string());
    let result = run_check(
        "docs context " + repo_path + " :: " + benchmark_case["query"].to_string(),
        bin_path,
        process_args,
    );
    json::parse(result["stdout"].to_string())["result"]
}

fn rank_of(results, wanted) {
    for entry in results {
        if entry["path"].to_string() == wanted.to_string() {
            return entry["rank"];
        }
    }
    ()
}

fn record_for(results, wanted) {
    for entry in results {
        if entry["path"].to_string() == wanted.to_string() {
            return entry;
        }
    }
    ()
}

fn relation_present(results, relation) {
    for entry in results {
        for step in entry["relation_path"] {
            if step["relation"].to_string() == relation.to_string() {
                return true;
            }
        }
    }
    false
}

fn currentness_of(record) {
    if record == () { return () }
    record["currentness"].to_string()
}

fn authority_of(record) {
    if record == () { return () }
    record["authority"]
}

fn evaluate_case(bin_path, repo_path, benchmark_case) {
    let payload = run_docs_context(bin_path, repo_path, benchmark_case);
    let results = payload["results"];
    let expect = benchmark_case["expect"].to_string();

    let expected_path = if "expected_path" in benchmark_case {
        benchmark_case["expected_path"].to_string()
    } else { () };
    let rival_path = if "rival_path" in benchmark_case {
        benchmark_case["rival_path"].to_string()
    } else { () };
    let forbidden_path = if "forbidden_path" in benchmark_case {
        benchmark_case["forbidden_path"].to_string()
    } else { () };

    let expected_record = if expected_path == () { () } else { record_for(results, expected_path) };
    let rival_record = if rival_path == () { () } else { record_for(results, rival_path) };
    let expected_rank = if expected_record == () { () } else { expected_record["rank"] };
    let rival_rank = if rival_record == () { () } else { rival_record["rank"] };
    let max_rank = benchmark_case["max_rank"];

    let relation_ok = if "expected_relation" in benchmark_case {
        relation_present(results, benchmark_case["expected_relation"].to_string())
    } else { true };
    let hops = if expected_record == () { () } else { expected_record["hops"] };

    let failures = [];
    if expect == "live-authority" {
        if expected_rank == () {
            failures.push("expected live authority `" + expected_path + "` is absent from the report");
        } else if expected_rank > max_rank {
            failures.push("expected live authority `" + expected_path + "` ranked "
                + expected_rank.to_string() + ", outside the top " + max_rank.to_string());
        }
        if rival_rank != () && expected_rank != () && rival_rank < expected_rank {
            failures.push("historical rival `" + rival_path + "` ranked "
                + rival_rank.to_string() + ", above the live authority at "
                + expected_rank.to_string());
        }
    } else if expect == "historical-retrieval" {
        if expected_rank == () {
            failures.push("directly named historical source `" + expected_path
                + "` was not retrieved");
        }
    } else if expect == "traversal" {
        if expected_rank == () {
            failures.push("traversed target `" + expected_path + "` is absent from the report");
        } else if hops != () && hops < 1 {
            failures.push("traversed target `" + expected_path
                + "` was reached lexically at 0 hops, not over a typed relation");
        }
        if !relation_ok {
            failures.push("relation `" + benchmark_case["expected_relation"].to_string()
                + "` never appears in a relation path");
        }
    } else if expect == "absent" {
        if rank_of(results, forbidden_path) != () {
            failures.push("unrelated high-authority document `" + forbidden_path
                + "` entered the report");
        }
    } else if expect == "empty" {
        if !results.is_empty() {
            failures.push("no-match query returned " + results.len().to_string() + " result(s)");
        }
    } else {
        failures.push("unknown expectation `" + expect + "`");
    }

    #{
        id: benchmark_case["id"].to_string(),
        dimension: benchmark_case["dimension"].to_string(),
        expect: expect,
        query: benchmark_case["query"].to_string(),
        expected_path: expected_path,
        expected_rank: expected_rank,
        expected_kind: if expected_record == () { () } else { expected_record["document_kind"].to_string() },
        expected_currentness: currentness_of(expected_record),
        expected_authority: authority_of(expected_record),
        expected_hops: hops,
        rival_path: rival_path,
        rival_rank: rival_rank,
        rival_currentness: currentness_of(rival_record),
        forbidden_path: forbidden_path,
        result_count: results.len(),
        applied_max_bytes: payload["budgets"]["applied"]["max_bytes"],
        context_bytes: payload["truncation"]["used_bytes"],
        truncated: payload["truncation"]["truncated"],
        profile_state: payload["profile"]["state"].to_string(),
        profile_fingerprint: payload["profile"]["fingerprint"].to_string(),
        scoped_documents: payload["profile"]["scoped_documents"],
        freshness_state: payload["freshness"]["state"].to_string(),
        passed: failures.is_empty(),
        failures: failures,
    }
}

fn render_case_line(case_result) {
    let line = "| `" + case_result["id"] + "` | " + case_result["dimension"];
    line += " | `" + case_result["query"] + "` | ";
    line += if case_result["expected_path"] == () { "-" } else { "`" + case_result["expected_path"] + "`" };
    line += " | " + if case_result["expected_rank"] == () { "-" } else { case_result["expected_rank"].to_string() };
    line += " | " + if case_result["expected_currentness"] == () { "-" } else { case_result["expected_currentness"] };
    line += " | " + if case_result["expected_authority"] == () { "-" } else { case_result["expected_authority"].to_string() };
    line += " | " + if case_result["rival_rank"] == () { "-" } else { case_result["rival_rank"].to_string() };
    line += " | " + case_result["context_bytes"].to_string();
    line += " | " + if case_result["passed"] { "pass" } else { "FAIL" } + " |";
    line
}

let index_lines = [
    "# Documentation context benchmark",
    "",
    "binary: " + bin_path,
    "generated_at: " + time::now_utc(),
    "",
];
let summary_targets = [];
let failed_cases = [];

for target in targets {
    let repo_path = target["repo"].to_string();
    if !fs::exists(path::join(repo_path, "effigy.toml")) {
        throw "[error] required benchmark target missing: " + repo_path;
    }

    run_graph_index(bin_path, repo_path);

    let case_results = [];
    for benchmark_case in target["cases"] {
        log("[info] docs-context benchmark " + target["label"].to_string()
            + " :: " + benchmark_case["id"].to_string());
        let case_result = evaluate_case(bin_path, repo_path, benchmark_case);
        if !case_result["passed"] {
            for failure in case_result["failures"] {
                failed_cases.push(target["label"].to_string() + "/" + case_result["id"] + ": " + failure);
            }
        }
        case_results.push(case_result);
    }

    let lines = [
        "# " + target["label"].to_string(),
        "",
        "repo: " + repo_path,
        "",
        "| case | dimension | query | expected source | rank | currentness | authority | rival rank | context bytes | result |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ];
    for case_result in case_results {
        lines.push(render_case_line(case_result));
    }
    let report_path = path::join(report_dir, target["label"].to_string() + ".md");
    fs::write_lines(report_path, lines);

    index_lines.push("- " + target["label"].to_string() + ": " + report_path);
    summary_targets.push(#{
        label: target["label"].to_string(),
        repo: repo_path,
        cases: case_results,
        report_path: report_path,
    });
}

let index_path = path::join(report_dir, "README.md");
fs::write_lines(index_path, index_lines);
let json_path = path::join(report_dir, "summary.json");
fs::write_file(
    json_path,
    json::stringify(#{
        binary: bin_path,
        generated_at: time::now_utc(),
        targets: summary_targets,
        failures: failed_cases,
    }),
);
log("[ok] wrote docs-context benchmark index " + index_path);
log("[ok] wrote docs-context benchmark summary " + json_path);

if !failed_cases.is_empty() {
    let detail = "";
    for failure in failed_cases {
        detail += "\n  - " + failure;
    }
    throw "[error] docs-context benchmark missed " + failed_cases.len().to_string()
        + " predeclared expectation(s):" + detail;
}
log("[ok] all predeclared docs-context expectations held");
