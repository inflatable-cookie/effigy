// Documentation-context retrieval benchmark.
//
// The corpus below is PREDECLARED. Queries, expected live authorities,
// historical rivals, and pass criteria are frozen in this file and committed
// before the benchmark is executed, so a green table cannot be manufactured by
// tuning a query after seeing a result. A miss is recorded and diagnosed, not
// edited away.
//
// History, so a reviewer can replay any state. Six freezes changed the matrix
// and carry the freeze discipline. One replay between freezes five and six
// changed no case. After freeze six, a second replay named that freeze commit
// without changing the matrix; that replay is not a seventh freeze.
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
//   b6a60e1b6  second freeze. Three Effigy cases were re-anchored on surfaces
//              that survive their own lane closing, and the no-match query was
//              replaced with terms that are genuinely absent. 10 of 11 held.
//              `effigy-current-roadmap` missed: `docs/roadmaps/g08/README.md`
//              ranked 6, not top three. Diagnosis: `roadmap` reaches 1540 of
//              2319 scoped documents, so corpus weighting drops it, and the
//              remaining terms (`generation`, `theme`, `purpose`) are section
//              boilerplate. Relevance ranks before currentness by contract, so a
//              lexically undiscriminating query is ordered by relevance noise.
//              The declared historical rival did not outrank the expected source;
//              only the top-three bound failed.
//   b05a7928a  third freeze. `effigy-current-roadmap` was replaced by
//              `effigy-current-over-archived`, because this repository has one
//              active milestone and this lane closes it, so no durable
//              current-roadmap anchor exists. 10 of 11 held. The new case
//              missed: archived guide `032` ranked 1 at relevance 48 and live
//              guide `039` ranked 2 at relevance 42. Diagnosis: the query
//              carried `consistency sweep`, which is the archived guide's own
//              title phrase, so it was a direct historical query rather than a
//              live-authority query. Contract `041` orders currentness only
//              between *similarly relevant* documents and relevance ranks first,
//              so ranking the more relevant historical document above the less
//              relevant current one is correct behavior, not a defect.
//   8c72bed4f  fourth freeze. The mis-specified live-authority slot is removed
//              rather than re-queried, and the behavior it actually demonstrated
//              is kept as an explicit direct-historical case. Current-versus-
//              historical ordering at genuinely equal relevance is proved in the
//              `generic-handbook` target, where two documents carry byte-
//              identical section text and differ only in state; that is the only
//              construction where relevance truly ties. The dated current-roadmap
//              result from the first freeze is preserved in the card `1090`
//              evidence log. No ranking rule, authority weight, or budget default
//              was changed at any point in this history.
//   892691f31  fifth freeze. `generic-no-match` added, and the `effigy-no-match`
//              literal confined to this file. That case asserts an empty report
//              against this repository's own corpus, whose profile roots include
//              docs/, README.md, AGENTS.md, CHANGELOG.md, and PAPERCUTS.md, so
//              documenting its query inside any of them gives the terms a
//              non-zero document frequency and turns the case red. It broke
//              exactly that way twice while the card `1090` evidence log was
//              being written. 12/12 hold.
//   this file  replay, after orchestrator review. Not a freeze: no case, query,
//              expected source, rival, or pass criterion changed; only the
//              script's expression shape, which had to shrink to parse under
//              Rhai's debug-build nesting cap. Replayed against the corpus
//              integrated with `main` at `db98139a9`: 12/12 hold, every rank
//              unchanged, two context-byte figures moved with the corpus.
//   ff95f6a4c  sixth freeze. Live `effigy-no-match` removed. Empty-result proof
//              is fixture-owned (`generic-no-match` only). The matrix rejects a
//              live-target empty case before any query runs, so documenting the
//              fixture query in Effigy profile roots cannot turn the proof red.
//              Current matrix: 11 cases. Historical freeze logs keep the older
//              12-case counts as historical evidence.
//   this file  replay, after review of the unnamed sixth-freeze marker. Not a
//              freeze and not a seventh freeze: no case, query, expected source,
//              rival, rank bound, dimension, or pass criterion changed. The
//              matrix remains the 11-case freeze at `ff95f6a4c`; only the
//              history now names that commit.
//   5da38abcb  replay for the docs-context latency lane (card `1113`). Not a
//              freeze: no case, query, expected source, rival, rank bound,
//              dimension, or pass criterion changed, and the matrix remains the
//              11-case freeze at `ff95f6a4c`. That commit compiles each
//              `docs_policy.graph` glob once per process; 11/11 held before and
//              after, every rank and every retrieved span unchanged, and the
//              whole run fell from 18833 ms to 12105 ms.
//   f3d0a0f6d  seventh freeze. Two exact-identifier cases added:
//              `effigy-exact-identifier` (`catalog_tasks` → guide `026`,
//              rank ≤ 3) and `generic-exact-identifier` (fixture identifier
//              `snorkel_grommet` with a split-word-dense rival). Existing
//              eleven cases and their rank bounds are unchanged. Current
//              matrix: 13 cases.
//   this file  eighth freeze. A third target, `portfolio-routing`, is added for
//              cross-repository source routing (roadmap `g09.006`, spec `122`).
//              It is a separate target with its own case shape because the
//              payload is `effigy.docs.context.sources.v1`, whose blocks are
//              grouped per repository and never ranked against each other. The
//              thirteen single-repository cases, their queries, expected
//              sources, rivals, rank bounds, and pass criteria are untouched,
//              and the two existing targets still run first. Current matrix:
//              13 single-repository cases + 9 portfolio cases.
//   this file  ninth freeze. `portfolio-membership-boundary` added after
//              review of PR `93`: `private-vault` now also declares a git
//              bundle, an include, and a local overlay that all claim
//              `share = true`, and the case asserts none of them grants
//              membership and that nothing was cloned or cached into that
//              checkout. No existing case, query, expected source, rival,
//              rank bound, or pass criterion changed. Current matrix: 13
//              single-repository cases + 10 portfolio cases.
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
// A third target, `portfolio-routing`, runs after those two. It materialises
// `tests/fixtures/docs-context-benchmark/portfolio` into a scratch directory,
// turns three of its children into real git checkouts, and mutates that copy to
// reach the states committed bytes cannot hold (a dirty file, a duplicate
// single-valued field). Every value it asserts - the status of each child, the
// grouping, the identity of each excerpt, and the exit rule - is declared in
// `portfolio_cases` below before the binary is run.
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
                // Fixture-owned empty proof. Naming this query in Effigy's
                // live profile roots is safe: the case does not target them.
                id: "generic-no-match",
                dimension: "no-match",
                expect: "empty",
                query: "quokka marmalade trombone",
                extra_args: [],
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
            #{
                id: "generic-exact-identifier",
                dimension: "exact-identifier",
                expect: "live-authority",
                query: "snorkel_grommet",
                extra_args: [],
                expected_path: "handbook/playbooks/coil-alignment.md",
                rival_path: "handbook/playbooks/snorkel-station.md",
                max_rank: 3,
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
            // Fourth freeze; see the freeze history above. A query that names an
            // archived guide by its own title still retrieves that guide, which
            // is the second half of the currentness contract: default ranking
            // prefers a current document over a *similarly relevant* historical
            // one, and never suppresses history a query asks for directly.
            #{
                id: "effigy-direct-historical-guide",
                dimension: "historical-decision",
                expect: "historical-retrieval",
                query: "docs consistency sweep and changelog",
                extra_args: [],
                expected_path: "docs/guides/archive/032-docs-consistency-sweep-and-changelog.md",
                max_rank: 8,
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
            #{
                id: "effigy-exact-identifier",
                dimension: "exact-identifier",
                expect: "live-authority",
                query: "catalog_tasks",
                extra_args: ["--max-sections", "3"],
                expected_path: "docs/guides/026-json-payload-examples.md",
                max_rank: 3,
            },
        ],
    },
];

// Build a string from an array of parts. Rhai caps expression nesting depth,
// and the cap is lower in debug builds than in release, so a long `a + b + c`
// chain parses under `target/release/effigy` and fails under
// `cargo run --bin effigy`. Concatenating in a loop keeps every expression
// shallow and makes the script parse identically in both profiles.
fn text(parts) {
    let out = "";
    for part in parts {
        out += part.to_string();
    }
    out
}

fn run_check(label, program, process_args) {
    let result = process::run(program, process_args);
    if result["success"] {
        return result;
    }
    throw text([
        "[error] ", label, " failed with status ", result["status"],
        "\nstdout:\n", result["stdout"],
        "\nstderr:\n", result["stderr"],
    ]);
}

fn run_graph_index(bin_path, repo_path) {
    log(text(["[info] indexing ", repo_path]));
    run_check(
        text(["graph index ", repo_path]),
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
        text(["docs context ", repo_path, " :: ", benchmark_case["query"]]),
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

fn optional_text(benchmark_case, key) {
    if key in benchmark_case {
        return benchmark_case[key].to_string();
    }
    ()
}

fn kind_of(record) {
    if record == () { return () }
    record["document_kind"].to_string()
}

fn rank_in(record) {
    if record == () { return () }
    record["rank"]
}

fn hops_in(record) {
    if record == () { return () }
    record["hops"]
}

// One checker per expectation. Each returns its own failure list, which keeps
// `evaluate_case` a flat sequence of guards instead of a deep `else if` chain -
// the shape that pushed the script past Rhai's in-function expression depth
// limit under a debug build.
fn live_authority_failures(expected_path, expected_rank, max_rank, rival_path, rival_rank) {
    let failures = [];
    if expected_rank == () {
        failures.push(text([
            "expected live authority `", expected_path,
            "` is absent from the report",
        ]));
        return failures;
    }
    if expected_rank > max_rank {
        failures.push(text([
            "expected live authority `", expected_path,
            "` ranked ", expected_rank,
            ", outside the top ", max_rank,
        ]));
    }
    if rival_rank == () {
        return failures;
    }
    if rival_rank < expected_rank {
        failures.push(text([
            "historical rival `", rival_path,
            "` ranked ", rival_rank,
            ", above the live authority at ", expected_rank,
        ]));
    }
    failures
}

fn historical_retrieval_failures(expected_path, expected_rank) {
    let failures = [];
    if expected_rank == () {
        failures.push(text([
            "directly named historical source `", expected_path,
            "` was not retrieved",
        ]));
    }
    failures
}

fn traversal_failures(expected_path, expected_rank, hops, relation_ok, relation) {
    let failures = [];
    if expected_rank == () {
        failures.push(text([
            "traversed target `", expected_path,
            "` is absent from the report",
        ]));
    }
    if expected_rank != () && hops != () && hops < 1 {
        failures.push(text([
            "traversed target `", expected_path,
            "` was reached lexically at 0 hops, not over a typed relation",
        ]));
    }
    if !relation_ok {
        failures.push(text([
            "relation `", relation,
            "` never appears in a relation path",
        ]));
    }
    failures
}

fn absent_failures(results, forbidden_path) {
    let failures = [];
    if rank_of(results, forbidden_path) != () {
        failures.push(text([
            "unrelated high-authority document `", forbidden_path,
            "` entered the report",
        ]));
    }
    failures
}

fn empty_failures(results) {
    let failures = [];
    if !results.is_empty() {
        failures.push(text([
            "no-match query returned ", results.len(), " result(s)",
        ]));
    }
    failures
}

fn reject_live_empty_cases(live_root, targets) {
    for target in targets {
        if target["repo"].to_string() != live_root.to_string() {
            continue;
        }
        for benchmark_case in target["cases"] {
            if benchmark_case["expect"].to_string() == "empty" {
                throw "[error] empty-result case `"
                    + benchmark_case["id"].to_string()
                    + "` targets the live repository; empty proof must use a fixture corpus";
            }
        }
    }
}

fn evaluate_case(bin_path, repo_path, benchmark_case) {
    let payload = run_docs_context(bin_path, repo_path, benchmark_case);
    let results = payload["results"];
    let expect = benchmark_case["expect"].to_string();

    let expected_path = optional_text(benchmark_case, "expected_path");
    let rival_path = optional_text(benchmark_case, "rival_path");
    let forbidden_path = optional_text(benchmark_case, "forbidden_path");
    let relation = optional_text(benchmark_case, "expected_relation");

    let expected_record = ();
    if expected_path != () {
        expected_record = record_for(results, expected_path);
    }
    let rival_record = ();
    if rival_path != () {
        rival_record = record_for(results, rival_path);
    }
    let expected_rank = rank_in(expected_record);
    let rival_rank = rank_in(rival_record);
    let hops = hops_in(expected_record);
    let max_rank = benchmark_case["max_rank"];

    let relation_ok = true;
    if relation != () {
        relation_ok = relation_present(results, relation);
    }

    let known = [
        "live-authority",
        "historical-retrieval",
        "traversal",
        "absent",
        "empty",
    ];
    let failures = [];
    if expect == "live-authority" {
        failures = live_authority_failures(
            expected_path, expected_rank, max_rank, rival_path, rival_rank,
        );
    }
    if expect == "historical-retrieval" {
        failures = historical_retrieval_failures(expected_path, expected_rank);
    }
    if expect == "traversal" {
        failures = traversal_failures(
            expected_path, expected_rank, hops, relation_ok, relation,
        );
    }
    if expect == "absent" {
        failures = absent_failures(results, forbidden_path);
    }
    if expect == "empty" {
        failures = empty_failures(results);
    }
    if !known.contains(expect) {
        failures.push(text(["unknown expectation `", expect, "`"]));
    }

    #{
        id: benchmark_case["id"].to_string(),
        dimension: benchmark_case["dimension"].to_string(),
        expect: expect,
        query: benchmark_case["query"].to_string(),
        expected_path: expected_path,
        expected_rank: expected_rank,
        expected_kind: kind_of(expected_record),
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

fn cell(value) {
    if value == () {
        return "-";
    }
    value.to_string()
}

fn quoted_cell(value) {
    if value == () {
        return "-";
    }
    text(["`", value, "`"]);
}

fn render_case_line(case_result) {
    let outcome = if case_result["passed"] { "pass" } else { "FAIL" };
    text([
        "| `", case_result["id"],
        "` | ", case_result["dimension"],
        " | `", case_result["query"],
        "` | ", quoted_cell(case_result["expected_path"]),
        " | ", cell(case_result["expected_rank"]),
        " | ", cell(case_result["expected_currentness"]),
        " | ", cell(case_result["expected_authority"]),
        " | ", cell(case_result["rival_rank"]),
        " | ", case_result["context_bytes"],
        " | ", outcome,
        " |",
    ])
}

let index_lines = [
    "# Documentation context benchmark",
    "",
    text(["binary: ", bin_path]),
    text(["generated_at: ", time::now_utc()]),
    "",
];
let summary_targets = [];
let failed_cases = [];

reject_live_empty_cases(repo_root, targets);

for target in targets {
    let repo_path = target["repo"].to_string();
    if !fs::exists(path::join(repo_path, "effigy.toml")) {
        throw text(["[error] required benchmark target missing: ", repo_path]);
    }

    run_graph_index(bin_path, repo_path);

    let case_results = [];
    for benchmark_case in target["cases"] {
        log(text([
            "[info] docs-context benchmark ", target["label"],
            " :: ", benchmark_case["id"],
        ]));
        let case_result = evaluate_case(bin_path, repo_path, benchmark_case);
        if !case_result["passed"] {
            for failure in case_result["failures"] {
                failed_cases.push(text([
                    target["label"], "/", case_result["id"], ": ", failure,
                ]));
            }
        }
        case_results.push(case_result);
    }

    let lines = [
        text(["# ", target["label"]]),
        "",
        text(["repo: ", repo_path]),
        "",
        "| case | dimension | query | expected source | rank | currentness | authority | rival rank | context bytes | result |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ];
    for case_result in case_results {
        lines.push(render_case_line(case_result));
    }
    let report_path = path::join(report_dir, text([target["label"], ".md"]));
    fs::write_lines(report_path, lines);

    index_lines.push(text(["- ", target["label"], ": ", report_path]));
    summary_targets.push(#{
        label: target["label"].to_string(),
        repo: repo_path,
        cases: case_results,
        report_path: report_path,
    });
}


// ---- Portfolio routing target -----------------------------------------------
//
// Frozen, like every case above: the expected statuses, identities, and exit
// codes are written here and committed before the run. The fixture is copied
// rather than queried in place because three of the states under test - a git
// checkout, a dirty file, and a duplicate single-valued field - cannot exist in
// a clean committed tree.

fn portfolio_scratch_setup(fixture_root, report_dir) {
    let source = path::join(fixture_root, "portfolio");
    let scratch = path::join(report_dir, "portfolio-scratch");
    fs::remove(scratch);
    fs::create_dir(scratch);
    run_check("copy portfolio fixture", "cp", ["-R", text([source, "/."]), scratch]);
    let repos = path::join(scratch, "repos");
    // Scrub any `.effigy` the source tree happens to be carrying, so the
    // membership-boundary case measures what this run did and not what some
    // earlier local command left behind.
    for repo_name in ["shared-atlas", "baseline-notes", "private-vault", "loose-notes"] {
        fs::remove(path::join(path::join(repos, repo_name), ".effigy"));
    }
    for repo_name in ["shared-atlas", "baseline-notes", "private-vault"] {
        let repo_path = path::join(repos, repo_name);
        run_check(text(["git init ", repo_path]), "git", ["-C", repo_path, "init", "-q"]);
        run_check(
            text(["git config ", repo_path]),
            "git",
            ["-C", repo_path, "config", "user.email", "fixture@example.invalid"],
        );
        run_check(
            text(["git config ", repo_path]),
            "git",
            ["-C", repo_path, "config", "user.name", "Effigy Fixture"],
        );
        run_check(text(["git add ", repo_path]), "git", ["-C", repo_path, "add", "-A"]);
        run_check(
            text(["git commit ", repo_path]),
            "git",
            ["-C", repo_path, "commit", "-qm", "portfolio fixture"],
        );
    }
    scratch
}

fn run_portfolio(bin_path, portfolio_path, query, extra_args, timeout_ms) {
    let program = bin_path.to_string();
    let process_args = [];
    if timeout_ms != () {
        program = "env";
        process_args.push(text(["EFFIGY_GRAPH_TIMEOUT_MS=", timeout_ms]));
        process_args.push(bin_path.to_string());
    }
    process_args.push("--json");
    process_args.push("docs");
    process_args.push("context");
    process_args.push(query.to_string());
    for extra in extra_args {
        process_args.push(extra.to_string());
    }
    process_args.push("--sources");
    process_args.push(portfolio_path.to_string());

    let result = process::run(program, process_args);
    let envelope = json::parse(result["stdout"].to_string());
    let payload = if result["success"] {
        envelope["result"]
    } else {
        json::parse(envelope["error"]["message"].to_string())
    };
    #{ success: result["success"], payload: payload }
}

fn portfolio_statuses(payload) {
    let out = [];
    for repository in payload["repositories"] {
        out.push(text([repository["handle"], "=", repository["status"]]));
    }
    out
}

fn portfolio_result_keys(payload) {
    let out = [];
    for repository in payload["repositories"] {
        for entry in repository["results"] {
            out.push(text([repository["handle"], "/", entry["path"]]));
        }
    }
    out
}

fn portfolio_identity(payload, handle, wanted) {
    for repository in payload["repositories"] {
        if repository["handle"].to_string() != handle.to_string() {
            continue;
        }
        for entry in repository["results"] {
            if entry["path"].to_string() == wanted.to_string() {
                return entry["content_identity"].to_string();
            }
        }
    }
    ()
}

fn expect_statuses(expected, actual) {
    let failures = [];
    if expected.len() != actual.len() {
        failures.push(text([
            "expected ", expected.len(), " repository block(s), got ", actual.len(),
            " (", actual, ")",
        ]));
        return failures;
    }
    let index = 0;
    while index < expected.len() {
        if expected[index].to_string() != actual[index].to_string() {
            failures.push(text([
                "block ", index, ": expected `", expected[index],
                "`, got `", actual[index], "`",
            ]));
        }
        index += 1;
    }
    failures
}

fn expect_exit(expected_success, actual_success) {
    let failures = [];
    if expected_success != actual_success {
        failures.push(text([
            "expected success=", expected_success, ", got success=", actual_success,
        ]));
    }
    failures
}

fn portfolio_case(id, dimension, detail, failures) {
    #{
        id: id,
        dimension: dimension,
        detail: detail,
        passed: failures.is_empty(),
        failures: failures,
    }
}

// Frozen expectations. `clean`, `dirty`, `duplicate-field`, and `timeout` name
// the fixture state each case runs in; the loop below reaches them in order and
// never goes back, so every state is entered exactly once.
let portfolio_clean_statuses = [
    "baseline-notes=ok",
    "loose-notes=invalid",
    "private-vault=not-shared",
    "shared-atlas=ok",
    "absent-directory=missing",
];
let portfolio_charter = "atlas/charters/tolerance-ledger.md";
let portfolio_notice = "atlas/notices/tolerance-ledger-withdrawn.md";
let portfolio_query = "tolerance ledger";
let portfolio_no_match_query = "quokka marmalade trombone";

let portfolio_scratch = portfolio_scratch_setup(fixture_root, report_dir);
let portfolio_path = path::join(portfolio_scratch, "portfolio.toml");
let portfolio_charter_path =
    path::join(path::join(path::join(portfolio_scratch, "repos"), "shared-atlas"), portfolio_charter);
let portfolio_results = [];

log("[info] docs-context benchmark portfolio-routing :: clean state");
let clean = run_portfolio(bin_path, portfolio_path, portfolio_query, [], ());
let clean_statuses = portfolio_statuses(clean["payload"]);
let clean_keys = portfolio_result_keys(clean["payload"]);

// 1. two-sided membership: one shared child answers, every other child is
//    reported with its own status, and nothing un-opted-in is searched.
let membership_failures = expect_statuses(portfolio_clean_statuses, clean_statuses);
for failure in expect_exit(true, clean["success"]) {
    membership_failures.push(failure);
}
portfolio_results.push(portfolio_case(
    "portfolio-membership", "two-sided-membership", text([clean_statuses]), membership_failures,
));

// 2. negative control: no result may come from a repository that never opted
//    in, from a skipped container, or from a hidden child.
let leak_failures = [];
let forbidden_owners = [
    "private-vault/", "loose-notes/", "decoy-checkout/", "hidden-annex/",
];
for key in clean_keys {
    for prefix in forbidden_owners {
        if key.to_string().starts_with(prefix.to_string()) {
            leak_failures.push(text([
                "un-opted-in or skipped source leaked into the report: ", key,
            ]));
        }
    }
}
portfolio_results.push(portfolio_case(
    "portfolio-negative-control", "no-leak", text([clean_keys.len(), " result(s)"]), leak_failures,
));

// 3. membership boundary: a child that never opted in carries a git bundle, an
//    include, and a local overlay that all claim `share = true`. None of them
//    may grant membership, and classifying it must not write into it - the
//    bundle cache would land in `<checkout>/.effigy`.
let vault_path = path::join(path::join(portfolio_scratch, "repos"), "private-vault");
let boundary_failures = [];
if fs::exists(path::join(vault_path, ".effigy")) {
    boundary_failures.push(
        "classification wrote `.effigy` into a repository that never opted in",
    );
}
for key in clean_keys {
    if key.to_string().starts_with("private-vault/") {
        boundary_failures.push(text(["never-opted-in source was searched: ", key]));
    }
}
let vault_status = ();
for repository in clean["payload"]["repositories"] {
    if repository["handle"].to_string() == "private-vault" {
        vault_status = repository["status"].to_string();
    }
}
if vault_status != "not-shared" {
    boundary_failures.push(text([
        "`private-vault` reported ", vault_status, ", not not-shared",
    ]));
}
portfolio_results.push(portfolio_case(
    "portfolio-membership-boundary", "committed-membership",
    text(["private-vault=", vault_status]), boundary_failures,
));

// 4. grouping: the same term is retrieved in both shared repositories, each
//    block ranks from 1, and no merged list exists.
let grouping_failures = [];
let shared_hit = false;
let baseline_hit = false;
for repository in clean["payload"]["repositories"] {
    if repository["results"].is_empty() {
        continue;
    }
    if repository["results"][0]["rank"] != 1 {
        grouping_failures.push(text([
            "block `", repository["handle"], "` starts at rank ",
            repository["results"][0]["rank"], ", not 1",
        ]));
    }
    if repository["handle"].to_string() == "shared-atlas" {
        shared_hit = true;
    }
    if repository["handle"].to_string() == "baseline-notes" {
        baseline_hit = true;
    }
}
if !shared_hit {
    grouping_failures.push("`shared-atlas` returned no section for the shared term");
}
if !baseline_hit {
    grouping_failures.push("`baseline-notes` returned no section for the shared term");
}
portfolio_results.push(portfolio_case(
    "portfolio-grouping", "grouped-not-merged",
    text([clean_keys]), grouping_failures,
));

// 5. identity: every result carries a span and every repository that answered
//    carries both HEADs; a clean checkout reports committed bytes.
let identity_failures = [];
for repository in clean["payload"]["repositories"] {
    if repository["results"].is_empty() {
        continue;
    }
    if repository["current_head"] == () {
        identity_failures.push(text(["block `", repository["handle"], "` has no current HEAD"]));
    }
    if repository["indexed_head"] == () {
        identity_failures.push(text(["block `", repository["handle"], "` has no indexed HEAD"]));
    }
    for entry in repository["results"] {
        if entry["content_identity"].to_string() != "committed" {
            identity_failures.push(text([
                "clean checkout reported `", entry["path"],
                "` as ", entry["content_identity"],
            ]));
        }
        if entry["span"]["end"]["byte"] <= entry["span"]["start"]["byte"] {
            identity_failures.push(text(["result `", entry["path"], "` has no span"]));
        }
    }
}
portfolio_results.push(portfolio_case(
    "portfolio-identity", "source-identity", "committed", identity_failures,
));

// 6. `--only`: a known handle answers, an unknown one is `disallowed`, and the
//    call still exits 0.
log("[info] docs-context benchmark portfolio-routing :: only");
let only = run_portfolio(
    bin_path, portfolio_path, portfolio_query,
    ["--only", "shared-atlas", "--only", "no-such-repo"], (),
);
let only_statuses = portfolio_statuses(only["payload"]);
let only_failures = expect_statuses(
    ["shared-atlas=ok", "no-such-repo=disallowed"], only_statuses,
);
for failure in expect_exit(true, only["success"]) {
    only_failures.push(failure);
}
portfolio_results.push(portfolio_case(
    "portfolio-only-disallowed", "disallowed", text([only_statuses]), only_failures,
));

// 7. a no-match query is a successful empty report per repository, never a
//    fallback to arbitrary files.
log("[info] docs-context benchmark portfolio-routing :: empty");
let empty = run_portfolio(bin_path, portfolio_path, portfolio_no_match_query, [], ());
let empty_statuses = portfolio_statuses(empty["payload"]);
let empty_failures = expect_statuses(
    [
        "baseline-notes=empty",
        "loose-notes=invalid",
        "private-vault=not-shared",
        "shared-atlas=empty",
        "absent-directory=missing",
    ],
    empty_statuses,
);
for failure in expect_exit(true, empty["success"]) {
    empty_failures.push(failure);
}
if !portfolio_result_keys(empty["payload"]).is_empty() {
    empty_failures.push("no-match query returned sections");
}
portfolio_results.push(portfolio_case(
    "portfolio-empty", "no-match", text([empty_statuses]), empty_failures,
));

// 8. a dirty file is never labelled as committed bytes, and its neighbours in
//    the same repository still are.
log("[info] docs-context benchmark portfolio-routing :: working tree");
fs::append_file(
    portfolio_charter_path,
    "\nAn uncommitted amendment to the reconciliation window.\n",
);
let dirty = run_portfolio(bin_path, portfolio_path, portfolio_query, [], ());
let dirty_failures = expect_statuses(portfolio_clean_statuses, portfolio_statuses(dirty["payload"]));
let dirty_identity = portfolio_identity(dirty["payload"], "shared-atlas", portfolio_charter);
let clean_neighbour = portfolio_identity(dirty["payload"], "shared-atlas", portfolio_notice);
if dirty_identity.to_string() != "working-tree" {
    dirty_failures.push(text([
        "dirty file reported as ", dirty_identity, ", not working-tree",
    ]));
}
if clean_neighbour != () && clean_neighbour.to_string() != "committed" {
    dirty_failures.push(text([
        "unchanged neighbour reported as ", clean_neighbour, ", not committed",
    ]));
}
portfolio_results.push(portfolio_case(
    "portfolio-working-tree", "source-identity",
    text([dirty_identity, "/", clean_neighbour]), dirty_failures,
));

// 9. a repository whose index carries failed paths answers as `stale`, keeps
//    its sections, and never blocks the healthy repository beside it.
log("[info] docs-context benchmark portfolio-routing :: stale");
fs::append_file(portfolio_charter_path, "\nState: live\n");
let stale = run_portfolio(bin_path, portfolio_path, portfolio_query, [], ());
let stale_failures = expect_statuses(
    [
        "baseline-notes=ok",
        "loose-notes=invalid",
        "private-vault=not-shared",
        "shared-atlas=stale",
        "absent-directory=missing",
    ],
    portfolio_statuses(stale["payload"]),
);
for failure in expect_exit(true, stale["success"]) {
    stale_failures.push(failure);
}
portfolio_results.push(portfolio_case(
    "portfolio-stale", "degraded-index",
    text([portfolio_statuses(stale["payload"])]), stale_failures,
));

// 10. when no repository answers, the call fails and still lists every status.
log("[info] docs-context benchmark portfolio-routing :: timeout");
let timed_out = run_portfolio(bin_path, portfolio_path, portfolio_query, [], 1);
let timeout_failures = expect_statuses(
    [
        "baseline-notes=timeout",
        "loose-notes=invalid",
        "private-vault=not-shared",
        "shared-atlas=timeout",
        "absent-directory=missing",
    ],
    portfolio_statuses(timed_out["payload"]),
);
for failure in expect_exit(false, timed_out["success"]) {
    timeout_failures.push(failure);
}
portfolio_results.push(portfolio_case(
    "portfolio-timeout", "timeout",
    text([portfolio_statuses(timed_out["payload"])]), timeout_failures,
));

for case_result in portfolio_results {
    if case_result["passed"] {
        continue;
    }
    for failure in case_result["failures"] {
        failed_cases.push(text(["portfolio-routing/", case_result["id"], ": ", failure]));
    }
}

let portfolio_lines = [
    "# portfolio-routing",
    "",
    text(["portfolio: ", portfolio_path]),
    "",
    "| case | dimension | observed | result |",
    "| --- | --- | --- | --- |",
];
for case_result in portfolio_results {
    let outcome = if case_result["passed"] { "pass" } else { "FAIL" };
    portfolio_lines.push(text([
        "| `", case_result["id"], "` | ", case_result["dimension"],
        " | `", case_result["detail"], "` | ", outcome, " |",
    ]));
}
let portfolio_report_path = path::join(report_dir, "portfolio-routing.md");
fs::write_lines(portfolio_report_path, portfolio_lines);
index_lines.push(text(["- portfolio-routing: ", portfolio_report_path]));
summary_targets.push(#{
    label: "portfolio-routing",
    repo: portfolio_scratch,
    cases: portfolio_results,
    report_path: portfolio_report_path,
});

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
log(text(["[ok] wrote docs-context benchmark index ", index_path]));
log(text(["[ok] wrote docs-context benchmark summary ", json_path]));

if !failed_cases.is_empty() {
    let detail = "";
    for failure in failed_cases {
        detail += text(["\n  - ", failure]);
    }
    throw text([
        "[error] docs-context benchmark missed ", failed_cases.len(),
        " predeclared expectation(s):", detail,
    ]);
}
log("[ok] all predeclared docs-context expectations held");
