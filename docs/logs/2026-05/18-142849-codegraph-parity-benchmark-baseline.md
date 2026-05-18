# CodeGraph Parity Benchmark Baseline

Date: 2026-05-18  
Roadmap: [`g07.036`](../../roadmaps/g07/036-parity-benchmark-harness-and-claim-discipline.md)  
Batch card: [`985`](../../roadmaps/g07/batch-cards/985-open-codegraph-parity-benchmark-lane.md)  
Strict lane: [`091`](../../specs/091-codegraph-parity-strict-lane.md)

## What Changed

- created the machine-readable gold query set:
  [`codegraph-parity-gold-queries.toml`](../../roadmaps/g07/codegraph-parity-gold-queries.toml)
- pinned the current warm-index baseline for live Effigy repo navigation
- separated exact-token fallback from graph-navigation measurements
- reserved deferred fixture-only cases for affected-test and cross-language work

## Benchmark Contract

The parity suite now measures against a fixed source file rather than
free-form prompts.

Current active corpus:

- ownership
- call-flow
- architecture
- freshness
- route and entrypoint proxy
- manifest behavior
- docs lookup
- exact-token fallback

Deferred fixture-backed corpus:

- affected-test proxy
- cross-language PHP flow

The deferred cases are intentionally pinned now even though the current runner
cannot execute them against ad hoc temp repos yet. That keeps later cards from
quietly rewriting the benchmark once feature work starts.

## Runner Rules

Warm-index baseline for this log:

1. `target/debug/effigy graph status --json`
2. confirm no stale paths
3. run `target/debug/effigy graph explore "<query>" --max-files 6 --max-bytes 12288 --json`
4. record:
   - elapsed time
   - top owner
   - whether the result supports zero reread, one targeted reread, or still
     needs `rg`

Exact-token baseline:

1. run `rg -n "<token>" crates src tests`
2. record elapsed time and hit count

No percentage claim is allowed from this baseline alone.

## Warm Index State

- graph ready: `true`
- stale paths: `0`
- indexed files: `3272`
- symbols: `31691`
- edges: `140479`
- references: `63527`

## Active Baseline Results

| Case | Query | Expected primary | Current top owner | Time | Current posture |
| --- | --- | --- | --- | ---: | --- |
| ownership | `trace deploy provider export` | `src/runner/deploy_command/provider_package.rs` | `src/runner/deploy_command/provider_package.rs` | `2.05s` | zero reread or one targeted reread |
| call-flow | `trace graph watch implementation` | `crates/effigy-codegraph/src/watch.rs` | `crates/effigy-codegraph/src/watch.rs` | `2.10s` | zero reread |
| architecture | `understand release orchestration` | `crates/effigy-release/src/lib.rs` | `crates/effigy-cli/src/command_parsing_release.rs` | `1.88s` | one targeted reread; acceptable alternate, but library owner is not yet top |
| freshness | `find graph status stale detection` | `crates/effigy-codegraph/src/index.rs` | `crates/effigy-codegraph/src/index.rs` | `2.80s` | zero reread or one targeted reread |
| route proxy | `where are task routes parsed` | `src/runner/execute/routing.rs` | `src/runner/execute/routing.rs` | `2.82s` | one targeted reread |
| manifest | `what changes when a bundle source is git` | `crates/effigy-manifest/src/bundles/source.rs` | `crates/effigy-manifest/src/bundles/source.rs` | `2.28s` | one targeted reread |
| docs | `docs for graph agent workflow` | `docs/guides/076-code-graph-and-agent-workflows.md` | `docs/guides/076-code-graph-and-agent-workflows.md` | `2.56s` | zero reread |

## Exact Token Fallback

| Case | Query | Command | Time | Result |
| --- | --- | --- | ---: | --- |
| exact token | `parse_task_selector` | `rg -n "parse_task_selector" crates src tests` | `0.02s` | `21` hits; exact lookup remains an `rg` workflow |

Top exact hits:

- `src/runner/tasks_command/status.rs`
- `crates/effigy-execution/src/lib.rs`
- `crates/effigy-codegraph/src/tests.rs`

## Deferred Cases

These remain in the gold query file but were not executed in this log:

- `affected-test-proxy`
  fixture source: `graph_context_ranks_tests_and_docs_when_request_intent_asks_for_them`
  blocker: no benchmark runner yet that materializes the temp fixture repo and
  evaluates changed-file or test-target output
- `cross-language-php-front-controller`
  fixture source: `graph_php_indexer_emits_namespace_symbols_and_static_include_edges`
  blocker: no fixture repo runner yet for cross-language parity cases

## Interpretation

- current `graph explore` is already strong on owner discovery for deploy,
  watch, graph freshness, bundle source behavior, and docs lookup
- the remaining active weakness in this corpus is broad release architecture,
  where CLI parsing still outranks the release library owner
- exact-token work still belongs to `rg`, and the benchmark now treats that as
  an explicit non-parity category instead of a hidden failure
- the next justified step remains `986`: source-body evidence should be indexed
  through FTS rather than read ad hoc during ranking

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `CONTRACT`, `MAINT`
- moved: the CodeGraph parity lane now has a fixed gold query corpus and a
  repeatable warm-index baseline
- remains open: FTS-backed ranking, traversal-aware explore, fixture-backed
  cross-language and affected-test execution, and final parity closeout

## Next Task

Execute `986`.
