# 678 - Split Deploy Text Rendering From Transaction State

Roadmap: [`../037-deploy-domain-boundary-hardening.md`](../037-deploy-domain-boundary-hardening.md)
Strict lane: [`../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md`](../../../specs/073-deploy-domain-boundary-hardening-strict-lane.md)
Contract: [`../../../contracts/029-deploy-domain-boundary-contract.md`](../../../contracts/029-deploy-domain-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move deploy human-facing text rendering out of transaction orchestration.

## Scope

- extract plan/apply/status/history text rendering helpers
- preserve existing text output exactly unless formatting changes are required
  by the move
- keep JSON report rendering unchanged
- keep transaction orchestration in `transaction.rs`

## Non-Goals

- no JSON schema changes
- no provider package dispatch changes
- no command behavior changes
- no new help text

## Acceptance

- `transaction.rs` no longer owns text rendering helpers
- render helpers consume report structs from the report owner
- deploy runner tests still pass
- text output remains stable enough for current tests and operators

## Outcome

- added `src/runner/deploy_command/text.rs`
- moved plan/apply/status/history text rendering helpers into the text owner
- kept JSON rendering and command orchestration unchanged
- reduced `transaction.rs` from 924 lines to 856 lines

## Validation

```sh
cargo test deploy_tests::
cargo check --bin effigy
git diff --check
```

## Next Task

Execute `679` to close deploy boundary proof.
