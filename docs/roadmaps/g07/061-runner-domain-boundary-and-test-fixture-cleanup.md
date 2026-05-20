# g07.061 - Runner Domain Boundary And Test Fixture Cleanup

Status: Complete
Depends on: `g07.060`

## Goal

Trim older runner and test harness duplication where ownership is clear, without
turning cleanup into a runner rewrite.

## Evidence

The audit found large runner files:

- `src/runner/script_command/mod.rs`
- `src/runner/state_command.rs`
- `src/runner/container_command/data.rs`
- `src/runner/db_seed.rs`

It also found repeated fixture setup across container, runtime, bootstrap, and
release tests.

## Scope

- inspect one runner surface at a time for pure domain logic that already has a
  natural crate owner
- extract only behavior with a clear boundary and tests
- add local fixture builders inside the affected test module or crate
- promote shared test helpers only after two crates need the exact same helper
- keep command behavior and error text stable unless fixing a proven issue

## Guardrails

- no broad runner architecture rewrite
- no new public crate just for theoretical test reuse
- no movement of shell/TTY/process glue into domain crates
- no weakening of release or container safety checks
- no fixture abstraction that hides the scenario being tested

## Suggested Implementation Shape

- start with the smallest obvious duplication in test fixtures
- then inspect `script_command/mod.rs` for separable parser/planner helpers
- only touch `state_command.rs` or container data if the boundary is obvious
- keep validation narrow per touched surface before running broader QA

## Acceptance Criteria

- at least one high-noise fixture pattern is replaced with a readable builder
- any runner extraction has a clear owner and no behavior drift
- touched tests remain easier to read than before
- no new cross-crate dependency cycle is introduced

## Next Task

No active ready card.
