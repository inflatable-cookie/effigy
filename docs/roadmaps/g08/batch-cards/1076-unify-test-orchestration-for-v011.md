# 1076 - Unify Test Orchestration For v0.11

Roadmap: [`../029-unified-test-orchestration-v011.md`](../029-unified-test-orchestration-v011.md)
Contracts: [`../../../contracts/001-working-rules.md`](../../../contracts/001-working-rules.md),
[`../../../contracts/038-unified-test-orchestration-contract.md`](../../../contracts/038-unified-test-orchestration-contract.md)
Spec: [`../../../specs/archive/102-unified-test-orchestration-v011.md`](../../../specs/archive/102-unified-test-orchestration-v011.md)

Status: Complete
Owner: Platform
Created: 2026-08-11

## Purpose

Deliver the approved v0.11 breaking test-orchestration contract in one
coherent command-surface batch.

## Work

- reject `tasks.test` after composed manifest loading with direct migration
- guarantee `test` reaches the built-in planner and `--plan` never executes
- preserve mixed supported-runner fanout and deterministic suite targeting
- widen configured suite `run` to managed run-step composition without adding
  a second execution engine
- migrate package script `test` into `[test.suites]`
- remove override language from runtime help, skills, starters, and guides
- add v0.11 breaking changelog coverage
- close the spec, roadmap, front doors, and evidence log after validation

## Acceptance

- [x] a marker-producing legacy override cannot run through `test --plan`
- [x] invalid override output names `[test.suites]` migration
- [x] package migration preview/apply never proposes `tasks.test`
- [x] configured command and managed-step suites plan and execute correctly
- [x] mixed Rust/Vitest detection remains aggregate
- [x] text/JSON plan and failure contracts pass
- [x] docs, skill mirrors, Clippy, and full QA pass

## Validation

- focused `effigy-manifest`, `effigy-tasks`, `effigy-builtin`, and runner tests
- exact marker-free regression for the reported papercut
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `effigy qa:docs`
- `effigy qa:ci:json`
- `effigy qa`
- graph affected analysis

## Stop Conditions

Stop if implementation needs a compatibility resolver, executes a suite during
planning, changes workflow files, or expands into unrelated test-framework
support.

## Next Task

Lane complete. Evidence:
[`11-144402-unified-test-orchestration-v011-closeout.md`](../../../logs/2026-08/11-144402-unified-test-orchestration-v011-closeout.md).
