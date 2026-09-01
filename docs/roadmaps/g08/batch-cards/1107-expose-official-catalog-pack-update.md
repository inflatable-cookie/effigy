# 1107 - Expose Official Catalog-Pack Update

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Blocked on card `1106`
Owner: Effigy official channel resolution and public update command
Created: 2026-09-01

## Purpose

Replace the placeholder official coordinate and expose explicit
`effigy service pack update` through the existing transaction.

## Acceptance

- `stable` resolves to a digest through the existing artifact boundary
- text/JSON/help report channel and resolved digest
- verified already-active digest is a deterministic no-op
- every resolution/pull/compatibility/validation/activation failure preserves
  active, previous, and channel metadata
- installed content cannot redirect the official coordinate
- ordinary commands remain network-silent; representative catalog workflows
  and recovery behavior regress unchanged

## Review Oracle

Reject mutable-tag activation, hidden coordinate override, state mutation on
failure/no-op, implicit registry probes, a second transport client, or a public
surface that cannot succeed against the card `1105` artifact.

## Stop Conditions

Stop if the official artifact is no longer public/attested/compatible, channel
resolution cannot return an immutable digest, or JSON compatibility would break.

## Next Task

Blocked. This card never authorizes an Effigy binary release.
