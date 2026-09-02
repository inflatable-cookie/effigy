# g08.048 Catalog-Pack Publication And Cutover

Status: Complete
Created: 2026-09-01
Spec: [`115`](../../specs/archive/115-catalog-pack-publication-and-cutover-strict-lane.md)
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
3. [`1105`](./batch-cards/1105-publish-first-official-catalog-pack.md) —
   Complete: preserved failed `v1.0.0`; published, attested, anonymously proved,
   and pointed `v1.0.1` plus `stable` at manifest digest `sha256:91de584e…`.
4. [`1106`](./batch-cards/1106-cut-over-generated-catalog-baseline.md) —
   Complete: Effigy's generated baseline and lock cut over from the accepted
   public artifact, with offline and public-artifact drift proof
   ([`02-144609`](../../logs/2026-09/02-144609-catalog-pack-generated-baseline-1106.md)).
   Merged as `6271b0ff129d006e47202b1b00def5ea7a395af8`.
5. [`1107`](./batch-cards/1107-expose-official-catalog-pack-update.md) —
   Complete: safe public update through the immutable official digest.
6. [`1108`](./batch-cards/1108-propose-generated-baseline-updates.md) —
   Complete: generated-only proposal automation plus a narrow provider
   checkpoint; the already-current digest correctly produced no dispatch.

The parallel frontier is integrated. The first non-empty proposal remains
future operational evidence when a new pack digest exists.

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

Run the operator intent checkpoint from vision `020`; do not infer an Effigy
release, S3 extraction, extension transport, or `g09` lane.
