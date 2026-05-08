# 421 - Implement Artifact Inspect Stage And Farmyard Handoff

Lane: [`042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md`](../042-artifact-substrate-for-seed-apply-and-capture-workflows-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-06

## Goal

Expose the first public artifact surface and a Farmyard-compatible handoff
output without adding live OCI transport yet.

## Scope

- add `effigy artifact inspect <REF|PATH>`
- add `effigy artifact stage <REF|PATH>`
- support local files and explicit `oci://` refs at the parsing/report layer
- stage local SQL-like payloads through `effigy-artifacts`
- emit JSON/text reports with `effigy.artifact.v1` metadata path, source,
  kind, staged root, primary files, and digest when known
- add an optional Farmyard handoff output shape for staged sources
- keep live OCI pull/push behind the existing adapter boundary for a later card

## Non-Goals

- no live private registry proof
- no dynamic OCI plugin loading
- no Acowtancy file edits
- no migration semantics in Effigy
- no `.github/workflows/` edits
- no release work

## Exit Condition

This card is complete when local artifact inspect/stage is available through a
public built-in command, JSON/text output is tested, and the Farmyard handoff
shape is explicit enough for Acowtancy adoption work.

## Next Task

Card
[`422-live-oci-transport-and-private-registry-proof.md`](./422-live-oci-transport-and-private-registry-proof.md).
