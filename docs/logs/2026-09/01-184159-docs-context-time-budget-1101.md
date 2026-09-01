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
  the repository walk. Warm and current queries never claim a refresh.
- `0` disables the bound for both graph and docs paths; the empty-query usage
  error path skips the progress probe so a doomed call never announces work.
- The shared bounded seam moved from `src/runner/graph_command.rs` into
  `src/runner/graph_time_budget.rs`; `run_graph` delegates to it unchanged in
  behavior, and `docs context` routes through the same seam with command
  identity `docs context`.
- One read-only freshness probe,
  `effigy_codegraph::refresh::refresh_pending`, was added to the existing
  refresh module. It mirrors `ensure_fresh`'s early exits (empty store, git
  skip-gate, scan-state walk) with the same primitives, so there is one
  freshness model and one refresh path.

## Applied Bound And Measured Bounded Result

Default bound: 120000 ms (`DEFAULT_GRAPH_TIMEOUT_MS`), unchanged. Probe bound
in the measured runs below: `EFFIGY_GRAPH_TIMEOUT_MS=1` (deliberately tiny).

Binary smoke on a 21-document fixture (cold store, no git repo), release of the
worker head's debug build:

| Run | Bound | Elapsed | Exit | stdout | stderr |
| --- | --- | --- | --- | --- | --- |
| docs context, cold, text | 1 ms | 53 ms | 1 | `effigy.graph.timeout.v1` detail | `[docs] docs context: graph index is missing; building the shared graph index before answering` + error block |
| docs context, cold, `--json` | 1 ms | 89 ms | 1 | valid `effigy.command.v1` envelope, `ok=false`, `error.details` = typed timeout | (no stdout claim) |
| docs context, stale, text | 1 ms | 52 ms | 1 | typed timeout, `command: "docs context"`, `timeout_ms: 1` | `[docs] docs context: graph index is stale; refreshing it before answering` |
| docs context, warm/current | 1 ms | - | 1 (timeout, expected at 1 ms) | typed timeout | no `[docs]` refresh claim |
| docs context, cold | 0 | 86 ms | 0 | full text report | refresh notice present (a cold build still announces; the bound is what `0` disables) |
| docs context, warm/current | 0 | 57 ms | 0 | full report | silent |
| graph search, cold, text | 1 ms | 34 ms | 1 | typed timeout, `command: "graph search"` | unchanged graph error block |

`graph search` at the same 1 ms bound measured 33-37 ms across three runs,
matching `docs context` at 34-53 ms: the docs path pays no extra bound overhead
beyond the probe.

## Review-Oracle Counterexamples

| # | Counterexample | Proof |
| --- | --- | --- |
| 1 | docs context blocks beyond a deliberately tiny bound on a cold graph | table above: 1 ms bound, cold and stale runs exit in 52-89 ms carrying `effigy.graph.timeout.v1`; focused tests `docs_cold_refresh_fails_within_a_deliberately_tiny_bound` and `docs_stale_refresh_fails_within_a_deliberately_tiny_bound` assert the same with a 5 s ceiling |
| 2 | JSON progress contaminates stdout or makes the envelope invalid | `--json` cold run stdout parses as `effigy.command.v1` with `ok=false` and `error.details.schema = effigy.graph.timeout.v1`; progress text goes to stderr only (`eprintln!` in `src/runner/docs_command/context.rs`); focused test `docs_cold_and_warm_queries_succeed_when_the_bound_is_disabled` additionally parses the success JSON stdout |
| 3 | timeout lacks the shared health snapshot or advertises different recovery | timeout detail still carries `health` (db path, size, index presence, refresh-in-progress) and the same three `next` steps including `effigy graph status --json`; asserted by `assert_typed_graph_timeout` in `docs_context_time_budget_tests.rs` for both `docs context` and `graph search` |
| 4 | warm/current queries emit a false refresh-progress message | warm run at bound 0 is stderr-silent; warm run at 1 ms times out with no `[docs]` claim; `refresh_notice_stays_silent_on_current_graph` asserts the decision returns `None` on a current graph |
| 5 | `0` differs between graph and docs consumers | both call the same `graph_time_budget()` (`None` when the parsed value is 0) and the same `run_bounded_graph_operation`; cold docs query at bound 0 succeeds (86 ms) exactly as unbounded graph queries do |
| 6 | graph command timeout behavior or schema changes incidentally | `graph_search_timeout_behavior_is_unchanged` asserts schema, `timeout_env`, `timeout_ms`, health object, and recovery for `graph search` on a cold fixture; all seven pre-existing `graph_tests.rs` regressions pass; the bounded helper moved but its body (worker clone, recv_timeout, disconnected handling, rendered fields) is byte-equivalent |

## Freshness Probe

`refresh_pending(repo_root)` in `crates/effigy-codegraph/src/refresh.rs`
returns `Current`, `Cold`, or `Stale` using the same early-exit order as
`ensure_fresh_with_wait`: empty store counts, then the git skip-gate, then the
scan-state walk. It mutates nothing beyond what every graph query already does
by opening the store. It is not a second refresh path; `ensure_fresh` remains
the only refresher and the probe only predicts it. A `Stale` verdict can still
be served by a concurrent refresher, which the refresh outcome already reports.

The notice decision lives in the docs shell (`refresh_progress_notice`) so the
library never writes stderr. Probe failures are silent: the retrieval itself
surfaces the real error. An empty or whitespace query skips the probe entirely
and keeps the existing `docs context requires a non-empty query` usage error.

## Files

- `src/runner/graph_time_budget.rs` — new shared seam: env parser, bounded
  operation, typed timeout detail (moved from `graph_command.rs`)
- `src/runner/graph_command.rs` — delegates to the shared seam; keeps
  `subcommand_is_bounded` and `graph_command_label`
- `src/runner/docs_command/context.rs` — probe, stderr notice, bounded routing
  with `docs context` identity; retrieval body unchanged
- `src/runner/docs_command/tests.rs` — notice decision tests
- `src/tests/runner_tests/runner_core_tests/docs_context_time_budget_tests.rs`
  — cold/stale/warm/disabled and graph-regression tests
- `crates/effigy-codegraph/src/refresh.rs`, `src/lib.rs` — read-only probe and
  re-export

## Validation

- `cargo test -p effigy --lib runner::tests::runner_core_tests::docs_context_time_budget_tests` — 4 passed
- `cargo test -p effigy --lib runner::docs_command::tests` — 3 passed
- `cargo test -p effigy --lib runner::tests::runner_core_tests::graph_tests` — 7 passed (pre-existing regressions)
- `cargo test -p effigy-codegraph` — 110 passed
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
