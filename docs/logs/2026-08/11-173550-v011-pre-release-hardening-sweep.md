# v0.11 Pre-Release Hardening Sweep

Status: complete
Created: 2026-08-11
Roadmap: operator-approved g08 release-readiness follow-up
Batch: v011-pre-release-hardening

## Summary
- closed the release-readiness gaps found by the 2026-08-11 codebase sweep
- kept release preparation and execution outside this batch

## Changes
- made Cargo test detection workspace-aware for both Nextest and plain Cargo
- moved Effigy's test, QA, and release gates onto the full Cargo workspace
- restored the cheap `health` -> mid-cost `validate` -> full `qa` ladder
- repaired the stale catalog assertion, docs-log next action, graph help, and
  formatting drift
- reused prepared affected-query indexes across validation-gap hotspots and
  made heuristic traversal follow `include_heuristic`
- centralized container test environment locking so plain Cargo tests do not
  race backend overrides
- reviewed and documented the unmaintained `smartstring` advisory inherited
  through Rhai; no safe upstream upgrade exists yet

## Vision Target Delta
- Primary tags: `ROUTE`, `CONTRACT`, `OPERATE`, `MAINT`, `RELEASE`
- Movement: partial root-package validation and contradictory orientation ->
  full workspace test authority, cheap health, current graph guidance, and
  green release-readiness checks
- Remaining gap: the reviewed Rhai/`smartstring` exception must be removed when
  upstream offers a maintained dependency path; three warning-only god-file
  findings remain deferred

## Validation Performed
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo nextest run --workspace --no-fail-fast` — 3,228 passed
- `cargo test --workspace --quiet`
- `cargo deny check`
- `./target/debug/effigy health`
- `./target/debug/effigy qa:docs`
- `./target/debug/effigy graph status --refresh --json`
- `./target/debug/effigy doctor --verbose` — `err:0`, one known god-file warning
- `./target/debug/effigy scan validation-gaps --json` — completed in about 12s,
  0 findings

## Risks
- `RUSTSEC-2026-0249` is unmaintained-only, not a vulnerability; the dated
  exception remains an explicit dependency-review obligation
- graph-native validation remains advisory and is not a release gate

## Next Task
- Await operator review of v0.11 readiness. No release mutation is implied.
