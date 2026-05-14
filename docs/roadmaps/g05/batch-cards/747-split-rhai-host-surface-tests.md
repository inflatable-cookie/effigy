# 747 - Split Rhai Host-Surface Tests

Roadmap: [`../026-rhai-host-surface-and-test-ownership.md`](../026-rhai-host-surface-and-test-ownership.md)
Strict lane: [`../../../specs/083-reusable-core-hardening-strict-lane.md`](../../../specs/083-reusable-core-hardening-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14
Completed: 2026-05-14

## Purpose

Make the Rhai host surface easier to maintain by splitting test ownership and
keeping docs aligned with the actual helper set.

## Scope

- split `effigy-rhai` tests by surface owner
- keep provider-facing helper proof easy to find
- refresh Rhai docs for YAML and any provider-relevant helper gaps proven by
  the tests

## Acceptance

- `crates/effigy-rhai/src/tests.rs` is no longer the primary god-file
- provider-facing Rhai helper coverage stays intact
- docs match the live structured-data helper surface

## Stop Conditions

- stop if the slice wants to widen the helper surface instead of documenting and
  testing it

## Result

- split `crates/effigy-rhai/src/tests.rs` into owned modules under
  `crates/effigy-rhai/src/tests/`
- kept shared Rhai fixtures, env helpers, and script-policy helpers in
  `tests/mod.rs`
- preserved provider-facing deploy/state/runtime proof coverage
- confirmed the Rhai test surface no longer appears in the god-file scan
- refreshed the first-party process allowlist to include the isolated underlay
  external bundle fixture path

## Validation

- `cargo test -p effigy-rhai`
- `cargo fmt --all -- --check`
- `effigy scan god-files --json`
- `git diff --check`

## Next Task

Execute `748`.
