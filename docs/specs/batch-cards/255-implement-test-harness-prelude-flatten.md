# 255 Implement Test-Harness Prelude Flatten

Status: landed
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Collapse the nested test-harness prelude chain so tests import their
fixtures from a single top-level prelude rather than routing through
three levels of `pub(in crate::runner::tests) use super::super::super::*`
shims.

## Context

Today, a test at
`src/tests/runner_tests/runner_core_tests/builtin_contract_tests/foo.rs`
pulls helpers via:

```
super::prelude::*
  → runner_core_tests/prelude.rs (re-export shim)
  → runner_tests/prelude.rs (multiple `pub(super) mod` facades)
  → test_support/ submodules
  → effigy_builtin::test_support::* (cross-crate)
```

Every level of that chain exists only to re-export. `pub(in crate::runner::tests)`
visibility on re-exports is awkward and doesn't actually flatten
names through `use super::*` in many cases — the post-`effigy-builtin`
extraction exposed this when `assert_parser_task_invocation_error`
had to grow an `E: Into<RunnerError>` bound just to thread
`BuiltinError` results through the chain.

Goal: replace the chain with a single `src/tests/prelude.rs` (or two
— one for runner tests, one for json-contract tests) that holds
every common helper as a direct `pub use`. Delete the intermediate
prelude modules.

## In Scope

- Create `src/tests/runner_tests/prelude.rs` as a single flat re-export
  surface covering everything the runner-side tests currently import
  from nested prelude modules.
- Delete the intermediate prelude files:
  - `src/tests/runner_tests/runner_core_tests/prelude.rs`
  - `src/tests/runner_tests/runner_core_tests/*/prelude.rs` (11 files)
  - `src/tests/runner_tests/*/prelude.rs` (10 additional files)
- Update every test file that imported from a nested prelude to import
  from the top-level one instead.
- If `json_contract_tests` needs its own prelude (because it doesn't
  share runner-test fixtures), keep it but flatten internally.
- Inside the new top-level prelude, re-export from
  `effigy_builtin::test_support::*`, `effigy_routing::discover_catalogs`,
  `effigy_tasks::parse_task_selector`, etc. — no more runner-side
  shim layer between the test and the upstream helper.
- Verify `src/runner/test_support.rs` can shrink or be deleted
  entirely after the test-side rewiring.

## Out Of Scope

- Test behavior changes. Every assertion, every fixture body stays
  identical.
- Renames of individual helpers (e.g. no `run_task_in_workspace`
  → `run_task` renames).
- Changes to test organization / directory structure beyond deleting
  prelude files.

## Acceptance Criteria

- At most two prelude files live anywhere under `src/tests/`
  (`runner_tests/prelude.rs`, optionally `json_contract_tests/prelude.rs`).
- No test-side prelude re-exports another prelude.
- `src/runner/test_support.rs` is either gone or a minimal surface
  that's justified independently.
- `cargo test --workspace` passes unchanged.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`
  (standard allowlist) both clean.

## Next Task

`g02.010` lane closes cleanly with this card. Hand the roadmap back
to planning for the next pivot — release closure resumption (card
`115`'s deferred execution path) or a fresh lane decision.
