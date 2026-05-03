# 025 - Test Module Extraction And Reorganization

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-03
Depends on: —

## Problem

`src/runner/mod.rs` uses fragile `#[path = "../tests/..."]` attributes to pull
test modules from outside its tree. Large test-only files live inside
`src/runner/`:

- `src/runner/system_command/workspace_tests.rs` — 1,073 lines
- `src/runner/container_command/gateway_registration_tests.rs` — 913 lines
- `src/runner/bootstrap_command/tests.rs` — 655 lines

These are not small inline unit tests; they are full test suites compiled into
the library crate. This increases library compile times and couples test
scaffolding to module privacy rules.

## Goal

Extract large test modules from `src/runner/` into proper integration tests or
`src/tests/` without fragile `#[path]` traversal.

## Scope

- move `workspace_tests.rs` and `gateway_registration_tests.rs` out of
  `src/runner/` into top-level `tests/` or `src/tests/`
- remove the `#[path]` attributes from `src/runner/mod.rs`
- adjust visibility or re-exports so tests can still reach the symbols they need
- verify that `cargo test` still passes after the move
- measure compile-time impact if possible

## Non-Goals

- rewriting test logic
- splitting tests into smaller files (can be a follow-up)
- changing the test framework

## Exit Condition

This milestone is complete when:

- no `#[path = "../tests/..."]` attributes remain in `src/runner/mod.rs`
- all moved tests still pass
- `cargo test` passes for the full workspace

## Next Task

If this lane is promoted, start by listing every `#[path]` attribute in
`src/runner/mod.rs` and the symbols each test module depends on.
