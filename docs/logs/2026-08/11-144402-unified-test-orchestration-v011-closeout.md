# Unified Test Orchestration v0.11 Closeout

Status: complete
Created: 2026-08-11
Roadmap: g08.029
Batch: card-1076-unify-test-orchestration-for-v011

## Summary

- Made `effigy test` the unconditional built-in test orchestrator.
- Rejected legacy `tasks.test` manifests with direct `[test.suites]`
  migration guidance through normal commands and doctor.
- Proved `effigy test --plan` cannot execute a marker-producing legacy task.
- Extended configured suite `run` values to the shared managed run-step grammar
  so task references and ordered commands stay under the test authority.
- Moved package-manager `test` migration into `[test.suites].js` while keeping
  ordinary scripts under `[tasks]`.
- Replaced dual-authority wording across help, starters, guides, and both
  bundled Effigy skill copies.
- Updated graph fixtures to expose configured test-suite symbols. A stale
  graph-context scan assertion now proves a missing index refreshes on demand.

## Vision Target Delta

- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `AGENT`
- Movement: baseline `test planning can route into an executable manifest task`
  -> current `one built-in orchestrator owns planning, selection, and execution`
- Remaining gap: None in this lane. Contract `038` owns the durable behavior.

## Behavior Evidence

- A manifest containing `[tasks.test]` fails before task routing and names the
  v0.11 suite migration.
- The regression fixture's execution marker remains absent after
  `effigy test --plan`.
- A configured suite plans an ordered task-ref/command chain without creating
  markers, then creates both markers during execution.
- Package migration preview and JSON identify `test.suites.js`; apply writes
  `[test.suites]` and preserves `package.json`.
- Code-graph affected/explore tests return named test-suite candidates instead
  of removed manifest test tasks.

## Validation Performed

- `effigy test cargo-nextest -- --no-fail-fast`
  - result: pass, 1,640 tests; 1 skipped
- `cargo clippy --all-targets -- -D warnings`
  - result: pass
- `effigy qa:docs`
  - result: pass
- `effigy qa:json`
  - result: pass
- focused manifest, migration, doctor, test-plan, managed-suite, help, status,
  scan, graph, and CLI-envelope regressions
  - result: pass
- `cargo fmt --all -- --check` and `git diff --check`
  - result: pass

## Boundaries

No workflow or release mutation ran. No new test framework detector, retry
policy, coverage aggregator, remote executor, or compatibility route was added.

## Next Task

Await explicit v0.11 release preparation. The implementation lane is complete;
do not infer release action.
