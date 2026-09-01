# Docs Context Cold Refresh Time Budget

Status: complete
Created: 2026-09-01
Roadmap: g08.046
Card: 1101
Contract: 041
Papercut: [`docs context` has no wall-clock bound on a cold graph](../../../PAPERCUTS.md)

## Summary

- `effigy docs context` lazy refresh now shares the graph command's one
  wall-clock budget: the `EFFIGY_GRAPH_TIMEOUT_MS` parser, the detached-worker
  bounded operation, the `effigy.graph.timeout.v1` typed detail, the health
  snapshot, and the recovery guidance. No second timeout model exists.
- A cold or stale refresh emits one concise progress notice on stderr before
  the rebuild walk. Warm and current queries never claim a refresh.
- All freshness detection lives inside the shared bound: the refresh pass
  reports its verdict through a progress callback
  (`RefreshPending::Cold`/`Stale`) derived from the same single freshness scan
  that feeds the rebuild, so no scan runs on the caller thread and no scan is
  repeated. The eager pre-bound probe from the first implementation was
  removed.
- Usage errors — empty query, invalid budgets — are validated on the caller
  thread through `effigy_codegraph::docs_context::validate_docs_context_request`
  before any bounded graph work, so a tiny bound can never pre-empt them. The
  same pure validation runs again inside the retrieval; there is one
  validation model.
- `0` disables the bound for both graph and docs paths.
- The shared bounded seam moved from `src/runner/graph_command.rs` into
  `src/runner/graph_time_budget.rs`; `run_graph` delegates to it unchanged in
  behavior, and `docs context` routes through the same seam with command
  identity `docs context`.

## Applied Bound And Measured Bounded Result

Default bound: 120000 ms (`DEFAULT_GRAPH_TIMEOUT_MS`), unchanged. Measured
runs below use `EFFIGY_GRAPH_TIMEOUT_MS=1` (deliberately tiny) and `0`
(disabled).

Binary smoke on a 21-document fixture (cold store, no git repo), rebuilt
debug binary at the post-repair head. Notice rows use bound `0` because with a
1 ms bound the process can exit before the worker thread reaches the notice;
the notice-before-walk ordering is asserted deterministically in-crate (see
Review Repair below).

| Run | Bound | Elapsed | Exit | stdout | stderr |
| --- | --- | --- | --- | --- | --- |
| docs context, cold, text | 1 ms | 47 ms | 1 | `effigy.graph.timeout.v1` detail | error block |
| docs context, cold, `--json` | 1 ms | 55 ms | 1 | valid `effigy.command.v1` envelope, `ok=false`, `error.details` = typed timeout | - |
| docs context, stale, text | 1 ms | 94 ms | 1 | typed timeout, `command: "docs context"`, `timeout_ms: 1`, `health` present | - |
| docs context, warm/current | 1 ms | - | 1 (timeout, expected at 1 ms) | typed timeout | no `[docs]` refresh claim |
| docs context, cold | 0 | 4.7 s (debug build) | 0 | full text report | `[docs] docs context: graph index is missing; building the shared graph index before answering` |
| docs context, warm/current | 0 | 113 ms | 0 | full report | silent |
| docs context, stale | 0 | 184 ms | 0 | full report | `[docs] docs context: graph index is stale; refreshing it before answering` |
| docs context, empty query | 1 ms | - | 1 | - | usage error `non-empty query`, no timeout |
| docs context, `--max-sections 0` | 1 ms | - | 2 | - | parse-layer usage error |
| graph search, cold, text | 1 ms | 34 ms | 1 | typed timeout, `command: "graph search"` | unchanged graph error block |

`graph search` at the same 1 ms bound measured 33-37 ms across three runs,
matching `docs context`: the docs path pays no extra bound overhead, and no
freshness scan runs before the bound starts.

## Review-Oracle Counterexamples

| # | Counterexample | Proof |
| --- | --- | --- |
| 1 | docs context blocks beyond a deliberately tiny bound on a cold graph | table above: 1 ms bound, cold and stale runs exit in 47-94 ms carrying `effigy.graph.timeout.v1`; focused tests `docs_cold_refresh_fails_within_a_deliberately_tiny_bound` and `docs_stale_refresh_fails_within_a_deliberately_tiny_bound` assert the same with a 5 s ceiling |
| 2 | JSON progress contaminates stdout or makes the envelope invalid | `--json` cold run stdout parses as `effigy.command.v1` with `ok=false` and `error.details.schema = effigy.graph.timeout.v1`; the notice goes to stderr only, from the bounded worker (`eprintln!` in `src/runner/docs_command/context.rs`); focused test `docs_cold_and_warm_queries_succeed_when_the_bound_is_disabled` additionally parses the success JSON stdout |
| 3 | timeout lacks the shared health snapshot or advertises different recovery | timeout detail still carries `health` (db path, size, index presence, refresh-in-progress) and the same three `next` steps including `effigy graph status --json`; asserted by `assert_typed_graph_timeout` in `docs_context_time_budget_tests.rs` for both `docs context` and `graph search` |
| 4 | warm/current queries emit a false refresh-progress message | warm run at bound 0 is stderr-silent; warm run at 1 ms times out with no `[docs]` claim; `refresh_notice_stays_silent_when_current` asserts the decision returns `None` for a current verdict; `refresh_progress_stays_silent_when_current` proves the refresh pass emits no callback on a current graph |
| 5 | `0` differs between graph and docs consumers | both call the same `graph_time_budget()` (`None` when the parsed value is 0) and the same `run_bounded_graph_operation`; cold docs query at bound 0 succeeds (4.7 s debug cold build) exactly as unbounded graph queries do |
| 6 | graph command timeout behavior or schema changes incidentally | `graph_search_timeout_behavior_is_unchanged` asserts schema, `timeout_env`, `timeout_ms`, health object, and recovery for `graph search` on a cold fixture; all seven pre-existing `graph_tests.rs` regressions pass; the bounded helper moved but its body (worker clone, recv_timeout, disconnected handling, rendered fields) is byte-equivalent |

## Review Repair

Review of head `81f7646c` requested changes on two execution misses; both are
repaired on this branch.

1. **Freshness walk outside the bound, then repeated.** The first
   implementation probed freshness on the caller thread
   (`refresh_progress_notice` → `refresh_pending` → `stale_paths_for_repo`)
   before the bound started, and `ensure_fresh` repeated the same walk inside
   the worker. Repaired by moving the verdict into the shared refresh pass:
   `refresh::ensure_fresh_with_progress` calls the progress callback with
   `Cold` (after the empty-store check, before `build_missing_index`) and with
   `Stale` (after the single staleness scan, before the rebuild). The scan
   that decides is the same scan that feeds the rebuild — the caller thread
   performs no graph work at all, and the eager probe is deleted.
   Deterministic proof in `crates/effigy-codegraph/src/docs_context/tests.rs`:
   `refresh_progress_reports_cold_before_the_build_walk` reads
   `status().ready == false` inside the callback (build has not run);
   `refresh_progress_reports_stale_before_the_rebuild_walk` reads
   `status().stale_paths` non-empty inside the callback and empty after return;
   `refresh_progress_stays_silent_when_current` asserts no callback at all.
2. **Timeout could pre-empt usage validation.** `run_context` now calls
   `docs_context::validate_docs_context_request` on the caller thread before
   spawning the bounded worker; empty-query and invalid-budget errors surface
   identically with the bound disabled or set to 1 ms. `docs_context_with_progress`
   runs the same pure validation first, so there is one validation model.
   Tiny-bound proofs: `empty_query_usage_error_wins_over_a_tiny_bound`
   (usage error text, `rendered_output() == None`) and
   `invalid_budget_usage_errors_win_over_a_tiny_bound` (zero and over-max
   budgets) in `docs_context_time_budget_tests.rs`; the CLI additionally
   rejects `--max-sections 0` at parse time.

The rebase onto current `main` (card `1102` merged as `83b6c9768`, card `1100`
merged as `e99356466`) is confined to `docs/logs/README.md`: all three sibling
log entries are kept. No code file overlaps with the merged lanes.

## Files

- `src/runner/graph_time_budget.rs` — new shared seam: env parser, bounded
  operation, typed timeout detail (moved from `graph_command.rs`)
- `src/runner/graph_command.rs` — delegates to the shared seam; keeps
  `subcommand_is_bounded` and `graph_command_label`
- `src/runner/docs_command/context.rs` — caller-side usage validation,
  verdict-to-notice mapping, bounded routing with `docs context` identity;
  retrieval body unchanged
- `src/runner/docs_command/tests.rs` — notice message decision tests
- `src/tests/runner_tests/runner_core_tests/docs_context_time_budget_tests.rs`
  — cold/stale/warm/disabled, usage-error, and graph-regression tests
- `crates/effigy-codegraph/src/refresh.rs`, `src/lib.rs` — progress verdict in
  the shared refresh pass and re-exports
- `crates/effigy-codegraph/src/docs_context/mod.rs` —
  `validate_docs_context_request`, `docs_context_with_progress`; selection,
  ranking, and budgets unchanged

## Validation

- `cargo test -p effigy --lib runner::tests::runner_core_tests::docs_context_time_budget_tests` — 6 passed (cold/stale/warm/disabled, graph regression, tiny-bound empty-query and invalid-budget usage errors)
- `cargo test -p effigy --lib runner::docs_command::tests` — 3 passed
- `cargo test -p effigy --lib runner::tests::runner_core_tests::graph_tests` — 7 passed (pre-existing regressions)
- `cargo test -p effigy-codegraph` — 117 passed (includes the three deterministic refresh-progress ordering proofs)
- `effigy graph affected` over the changed sources (`ok=true`): doc-path
  reachability from the changed roadmap/card/log files fans out to 100 files,
  so the meaningful rust targets stay the changed crates themselves; direct
  targets ran as the focused suites above plus `cargo test -p effigy-codegraph`
- `effigy qa` — passed (test suites, docs checks, JSON contract checks)
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `git diff --check`

## Next Task

Return the exact-head PR to the Effigy orchestrator. Shared PAPERCUTS,
changelog, contract, and guide closeout stay with serial merge.
