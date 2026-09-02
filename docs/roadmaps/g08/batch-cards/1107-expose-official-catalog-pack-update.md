# 1107 - Expose Official Catalog-Pack Update

Roadmap: [`../048-catalog-pack-publication-and-cutover.md`](../048-catalog-pack-publication-and-cutover.md)
Spec: [`../../../specs/archive/115-catalog-pack-publication-and-cutover-strict-lane.md`](../../../specs/archive/115-catalog-pack-publication-and-cutover-strict-lane.md)
Contract: [`043`](../../../contracts/043-feature-placement-and-surface-migration-contract.md)

Status: Complete
Owner: Effigy official channel resolution and public update command
Created: 2026-09-01
Promoted: 2026-09-02 — card `1106` merged at `6271b0ff129d006e47202b1b00def5ea7a395af8`
Closed: 2026-09-02 — public `service pack update` resolves compiled `stable` to
the accepted immutable digest and reuses the existing transaction; evidence
[`02-155016`](../../../logs/2026-09/02-155016-official-catalog-pack-update-1107.md)

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

## Validation

- unit and integration counterexamples for mutable-tag input, coordinate
  override, verified no-op, and failure atomicity
- CLI text, help, JSON-envelope, and exit-contract coverage for `service pack
  update`
- live read-only resolution of public `stable` to the accepted immutable
  `sha256:91de584e…` digest, followed by an isolated-home end-to-end update and
  repeated no-op proof
- representative baseline, installed-pack, rollback/reset, service,
  container, system, workspace, bootstrap, and ordinary-network-silence tests
- `effigy qa`, formatting, clippy with warnings denied, and diff checks

## Evidence

[`02-155016`](../../../logs/2026-09/02-155016-official-catalog-pack-update-1107.md)
maps every acceptance and review-oracle row to named tests plus the isolated
live smoke (channel, digest, store before/after success and no-op, network
silence). Failure atomicity is the runner unit table in that log. No Effigy
release is part of the log.

## Review Oracle

Reject mutable-tag activation, hidden coordinate override, state mutation on
failure/no-op, implicit registry probes, a second transport client, or a public
surface that cannot succeed against the card `1105` artifact.

## Stop Conditions

Stop if the official artifact is no longer public/attested/compatible, channel
resolution cannot return an immutable digest, or JSON compatibility would break.

## Next Task

Card `1108` and the shared lane closeout are complete. This card never
authorizes an Effigy binary release.
