# g08.044 Rhai Storage Create-Only

Status: Complete
Created: 2026-09-01
Completed: 2026-09-01
Evidence: [`../../logs/2026-09/01-182838-rhai-storage-create-only-1099.md`](../../logs/2026-09/01-182838-rhai-storage-create-only-1099.md)
Spec: [`114`](../../specs/archive/114-rhai-storage-create-only-strict-lane.md)
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
- [x] [`1099`](./batch-cards/1099-add-rhai-storage-create-only.md) — complete

## Acceptance

- exactly one of two same-key create-only requests wins
- the loser cannot replace the winner's bytes or metadata
- existing unconditional writes remain unchanged
- the runtime, surface catalog, guide, and changelog agree
- focused and full Effigy validation pass

## Next Task

Pass card `1099`'s PR through exact-head orchestrator review. After merge, the
orchestrator resumes the preserved Bovine PR 32 worker and returns the queue to
official catalog-pack publication planning under contract `043`.
