# g08.048 Catalog-Pack Publication And Cutover

Status: Active
Created: 2026-09-01
Spec: [`115`](../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Architecture: [`026`](../../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Establish the external catalog-pack ownership boundary without adding operator
ceremony, surprise network access, or a second release authority.

## Sequence

1. [`1103`](./batch-cards/1103-establish-catalog-pack-support-floor.md) —
   Complete: commit Effigy's compatibility authority.
2. [`1104`](./batch-cards/1104-build-catalog-pack-repository-foundation.md) —
   Complete: dedicated public pack repository, exact import, validation, and
   no-push publication rehearsal.
3. [`1105`](./batch-cards/1105-publish-first-official-catalog-pack.md) — Ready:
   preserve the failed pre-push `v1.0.0` source tag, then publish and prove
   `v1.0.1` and `stable` at one verified digest under the explicit 2026-09-02
   recovery authority.
4. [`1106`](./batch-cards/1106-cut-over-generated-catalog-baseline.md) — blocked
   on accepted publication evidence: generate Effigy's baseline and lock, then
   prove offline and public-artifact drift.
5. [`1107`](./batch-cards/1107-expose-official-catalog-pack-update.md) — blocked
   on `1106`: replace the placeholder coordinate and expose safe public update.
6. [`1108`](./batch-cards/1108-propose-generated-baseline-updates.md) — blocked
   on `1106`: enable the narrow GitHub App proposal path.

`1107` and `1108` form the first potential parallel frontier. All earlier edges
are real dependency or operator-mutation gates. Same-repository merges remain
serial.

## Acceptance

- one canonical pack source and one generated Effigy recovery snapshot
- independently versioned public artifact with deterministic digest identity
- non-vacuous Effigy-owned compatibility authority
- no implicit registry access or activation
- first public update succeeds and is transactional/no-op safe
- proposal automation has generated-only, non-merge authority
- every mutation and rollback boundary has recorded evidence

## Non-Goals

- Effigy binary release
- automatic publication from `main` or tag push
- automatic installed-pack pruning
- parallel compatibility channels
- S3 or general extension transport
- `g09` rollover

## Next Task

Resume card `1105` on its existing worker lane for the bounded `v1.0.1` repair
PR. Cards `1106` through `1108` remain blocked exactly as named above; the
repair must merge before the new source tag or any package, attestation, or
`stable` mutation.
