# g08.044 Rhai Storage Create-Only

Status: Ready
Created: 2026-09-01
Spec: [`114`](../../specs/114-rhai-storage-create-only-strict-lane.md)
Contract: [`044`](../../contracts/044-rhai-storage-create-only-contract.md)
Consumer blocker: [Bovine PR 32](https://github.com/acowtancy/bovine-accelerator/pull/32#issuecomment-5497389670)

## Purpose

Add atomic exclusive-create semantics to the retained Rhai S3 PutObject
surface, unblocking Bovine's fail-closed upload collision repair.

## Scope

- additive `storage::put(create_only: true)` parsing and request wiring
- stable redacted precondition-failure diagnostic
- local deterministic request/collision proof
- Rhai surface inventory, focused user guidance, changelog, and evidence

## Boundary

- no live S3 or consumer mutation
- no removal, extraction, provider framework, arbitrary conditional headers,
  retry policy, release, workflow, or catalog-pack publication work
- Bovine remains a separate downstream PR and merge

## Card

- [ ] [`1099`](./batch-cards/1099-add-rhai-storage-create-only.md) — ready

## Acceptance

- exactly one of two same-key create-only requests wins
- the loser cannot replace the winner's bytes or metadata
- existing unconditional writes remain unchanged
- the runtime, surface catalog, guide, and changelog agree
- focused and full Effigy validation pass

## Next Task

Run card `1099`, merge after exact-head review, then resume the preserved
Bovine PR 32 worker.
