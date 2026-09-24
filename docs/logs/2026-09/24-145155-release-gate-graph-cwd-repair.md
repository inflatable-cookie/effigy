# Release Gate Graph CWD Repair

Status: complete
Created: 2026-09-24
Roadmap: 0.13.0 release readiness
Batch: runtime-drift-guard-repair

## Summary

- Exact-head hosted CI passed for `ec230a8ec936a3dc449b308fb523d435beacd96f`; the release gate runner then passed `ci`, `format`, and `test` but failed `qa` on the runtime drift guard before `build`, `smoke`, or `metadata` ran.
- `src/runner/graph_command.rs` read `std::env::current_dir()` when selecting a catalog scope. It now carries the captured invocation cwd from the existing command context through normal and bounded graph execution.
- Added a regression in which an embedded invocation from a catalog member selects that member even when the process cwd is elsewhere, plus a `[Unreleased]` fix entry.

## Vision Target Delta

- Primary tags: `RELEASE`, `CONTRACT`, `ROUTE`.
- Movement: release QA blocked by direct runner cwd access -> graph selection uses the captured runtime context and drift guard passes.
- Remaining gap: new exact-head CI and all release gates must pass before 0.13.0 preparation.

## Validation Performed

- `effigy qa:architecture:runtime-container-drift`: passed.
- `cargo test --lib graph_catalog_tests`: four passed, including captured-cwd regression.
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --all -- --check`,
  and `effigy changelog validate`: passed.
- `effigy qa:docs`: passed, including links, examples, indexes, headings,
  workflow paths, and vision next-action checks.
- `git diff --check`: passed.

## Next Task

Re-establish exact-head CI and rerun all release gates for the repaired commit. Then review the 0.13.0 release preparation preview and support-policy update.
