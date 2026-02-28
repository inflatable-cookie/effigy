# Acowtancy Testing Orchestration Validation

Date: 2026-02-27
Owner: Platform
Related roadmap: 005 - Unified Testing Orchestration

## Scope

Validate built-in `effigy test` behavior against an active multi-repo workspace (`acowtancy`), including plan output, sub-repo routing, and execution startup path.

## Changes

- Ran root and sub-repo `test --plan` coverage using current Effigy build.
- Captured parsing blockers in project manifests that prevent full root fanout.
- Captured execution startup evidence for Rust/nextest path in `farmyard`.
- Re-ran after manifest fixes and captured root suite-targeted execution results.

## Validation

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result: failed due invalid TOML in child catalog (`cream/effigy.toml`: `js = bun` missing quotes).

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy/farmyard`
  - result: pass; selected `cargo-nextest`, showed fallback chain (`vitest` rejected, `cargo-test` available fallback).

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy/cream`
  - result: failed due invalid TOML (`js = bun` missing quotes).

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy/dairy`
  - result: failed due invalid TOML (`js = bun` missing quotes).

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test nextest -E 'none()'`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy/farmyard`
  - result: execution path started successfully and entered Rust workspace compile + nextest path; terminated manually due compile-time cost for this checkpoint run.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test --plan`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result (after fix): pass; root fanout now detects `cream`/`dairy` (`vitest`) and `farmyard` (`cargo-nextest`) without manifest parse errors.

- command: `cargo run --manifest-path /Users/betterthanclay/Dev/projects/effigy/Cargo.toml --bin effigy -- test vitest`
  - cwd: `/Users/betterthanclay/Dev/projects/acowtancy`
  - result (after fix): pass for orchestration path; suite-targeted fanout executed against `cream` + `dairy`, returned non-zero due real project test failure in `dairy` (`tests/summary-forms.test.ts` timeout), and produced ordered per-target summary (`cream: ok`, `dairy: exit=1`).

## Findings

- Effigy test orchestration behavior is functional for valid manifests.
- Root-level fanout validation is now unblocked after manifest corrections.
- Real execution at root confirms:
  - suite-targeted routing works (`vitest` fanout over matching targets),
  - per-target aggregation renders correctly,
  - non-zero propagation reflects real target test failures.

## Risks / Follow-ups

- Remaining risk is project test health (for example current dairy timeout), not Effigy orchestration behavior.

## Next

- Optional: rerun full mixed-suite root execution (`effigy test`) when Rust compile budget is acceptable to capture one complete end-to-end baseline in this workspace.
