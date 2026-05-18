# Fixture Backed Parity Proof

Date: 2026-05-18  
Roadmap: [`g07.048`](../../roadmaps/g07/048-fixture-backed-parity-proof.md)  
Batch card: [`998`](../../roadmaps/g07/batch-cards/998-add-fixture-backed-parity-runner.md)  
Strict lane: [`092`](../../specs/092-codegraph-parity-follow-up-strict-lane.md)

## What Changed

- added shared test-fixture builders for the deferred graph-watch and PHP
  front-controller parity cases
- reused those builders in the existing graph tests instead of keeping a second
  parity-only fixture shape
- added a bounded parity-runner test that materializes each fixture in a temp
  repo, indexes it, runs `explore`, and asserts the primary owner stays inside
  the pinned gold-query acceptance window
- promoted the two deferred fixture cases in the gold query file into active
  fixture-backed cases

## Fixture Results

| Case | Query | Result | Top owner |
| --- | --- | --- | --- |
| `affected-test-proxy` | `graph watch regression tests` | exact | `tests/graph_watch_tests.rs` |
| `cross-language-php-front-controller` | `trace php front controller boot helper` | acceptable alternate | `legacy/App/Controller.php` |

## Interpretation

- the fixture-backed cases are no longer placeholders; they now run from real
  temp repos inside the test suite
- the graph-watch proxy behaves as intended and lands directly on the test file
- the PHP cross-language case is runnable but still prefers the controller file
  over the front controller entrypoint, so closeout must keep that as an
  explicit residual weakness rather than claiming full parity

## Validation

- `cargo test -p effigy-codegraph graph_context_ranks_tests_and_docs_when_request_intent_asks_for_them -- --nocapture`
- `cargo test -p effigy-codegraph graph_php_indexer_emits_namespace_symbols_and_static_include_edges -- --nocapture`
- `cargo test -p effigy-codegraph graph_deferred_parity_fixture_cases_are_runnable -- --nocapture`

## Vision Target Delta

- primary vision tags touched: `OPERATE`, `MAINT`
- moved: the last deferred parity cases now execute from bounded temp fixtures
  instead of living only as benchmark placeholders
- remains open: final follow-up closeout and the explicit parity posture call

## Next Task

Execute `999`.
