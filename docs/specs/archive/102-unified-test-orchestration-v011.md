# 102 - Unified Test Orchestration v0.11

Roadmap: [`g08.029`](../../roadmaps/g08/029-unified-test-orchestration-v011.md)
Contract: [`038`](../../contracts/038-unified-test-orchestration-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-11
Completed: 2026-08-11

## Purpose

Remove the split authority between built-in test orchestration and
`tasks.test`. Make the central test command safer and simpler than direct
runner invocation for humans and agents.

## Lane Posture

Posture: `strict-complete`

Completed card:

- [`1076`](../../roadmaps/g08/batch-cards/1076-unify-test-orchestration-for-v011.md)

## Settled Decisions

- v0.11 is the approved breaking release for this change.
- `effigy test` is always built-in.
- `[test]` is the sole configuration authority.
- `tasks.test` is rejected with a direct migration message; no compatibility
  shim remains.
- `--plan` is a no-execution invariant.
- auto-detection covers every supported ecosystem present per catalog root.
- configured suites provide arbitrary names and managed run-step composition.

## Acceptance

- [x] legacy override rejection and migration are explicit
- [x] `test --plan` is proven marker-free for the reported failure shape
- [x] mixed Rust/TypeScript selection stays aggregate by default
- [x] suite selection and catalog selection are deterministic
- [x] suite run-step flexibility replaces the legitimate override use cases
- [x] skills, starters, guides, help, and changelog expose one rule
- [x] focused tests, docs, JSON contracts, Clippy, and full QA pass

## Stop Conditions

No stop condition triggered. The implementation did not add compatibility
routing or execute suite work during planning.

## Promotion State

The durable rule is promoted to contract `038`. This archived spec records the
completed v0.11 implementation lane.

## Evidence

- [`11-144402-unified-test-orchestration-v011-closeout.md`](../../logs/2026-08/11-144402-unified-test-orchestration-v011-closeout.md)

## Next Task

Lane complete. No release action is implied.
