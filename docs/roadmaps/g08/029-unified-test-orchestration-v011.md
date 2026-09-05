# g08.029 - Unified Test Orchestration v0.11

Status: Complete
Depends on: `g08.028`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`038`](../../contracts/038-unified-test-orchestration-contract.md)
Spec: [`102`](../../specs/archive/102-unified-test-orchestration-v011.md)

## Goal

Make `effigy test` the unambiguous, safe, polyglot test front door by removing
the competing `tasks.test` route and consolidating configuration under
`[test]`.

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`, `AGENT`
- Target envelope: test selection is deterministic, inspectable, and easier
  than remembering language-specific runner commands.
- Vision target delta: `--plan` becomes a hard read-only boundary and every
  repository has one test-orchestration authority.

## Execution Plan

- [x] card 1076: cut runtime/config precedence, migration, suite flexibility,
      public guidance, proof, and lane closeout

## Non-Goals

- no new test framework detector beyond current Rust and Vitest support
- no remote execution, flaky retry policy, coverage aggregation, or CI rewrite
- no release mutation or workflow edit
- no compatibility mode for `tasks.test`

## Acceptance Criteria

- [x] contract `038` is implemented without dual routing
- [x] both recorded `test --plan` papercut shapes are prevented
- [x] package-script migration emits `[test.suites]`
- [x] mixed-language and explicit-suite plans explain exact execution
- [x] public docs and agent guidance contain one test rule
- [x] v0.11 breaking changelog entry is present
- [x] focused and full project validation pass

## Evidence

- [`11-144402-unified-test-orchestration-v011-closeout.md`](../../logs/archive/2026-08/11-144402-unified-test-orchestration-v011-closeout.md)

## Next Task

Lane complete. Contract `038` owns the durable behavior; no release action is
implied.
