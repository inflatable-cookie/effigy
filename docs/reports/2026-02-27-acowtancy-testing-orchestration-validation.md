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

## Findings

- Effigy test orchestration behavior is functional for valid manifests.
- Root-level fanout validation in Acowtancy is currently blocked by invalid TOML in child manifests:
  - `/Users/betterthanclay/Dev/projects/acowtancy/cream/effigy.toml`
  - `/Users/betterthanclay/Dev/projects/acowtancy/dairy/effigy.toml`
- Required fix in affected manifests:
  - `js = "bun"` (string) instead of `js = bun`.

## Risks / Follow-ups

- Until child manifest parse errors are fixed, root-level `effigy test` fanout in Acowtancy will fail before orchestration.
- A short rerun should be performed immediately after manifest correction to close roadmap acceptance cleanly.

## Next

- Fix invalid `js` values in Acowtancy child manifests and rerun:
  1. `effigy test --plan` at Acowtancy root
  2. one full `effigy test` root run (or suite-targeted run) to confirm end-to-end fanout and summary
