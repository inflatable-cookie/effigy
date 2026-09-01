# Feature Boundary Residual Open Design

Status: open
Created: 2026-09-01
Owner: orchestrator
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Preserve feature-placement questions that remain unresolved after catalog-pack
acquisition decisions moved into architecture `026`, contract `043`, and the
`g08.040` prototype lane.

## Open Questions

- What extension transport should optional runtime/provider code use?
- What is the minimum base Rhai surface after provider-specific helpers move?
- What exact consumer evidence will show the
  `bovine-accelerator-desktop` media-upload replacement is live and the
  `bovine-accelerator` Rhai storage dependency can retire?
- What evidence threshold should justify a future provider implementation in
  mandatory core?

## Settled Boundaries

- Semantic ownership, not universality or facade exposure, decides core.
- Help-first discovery shipped under card `1093`; direct execution routes and
  selector precedence remain unchanged.
- Repository intelligence remains core.
- Catalog-pack acquisition decisions are promoted and must not be reopened from
  this note.
- Release transaction safety remains core while Effigy-specific distribution
  recipes move outward.
- S3 remains supported until the named consumer replacement gate is proved.
- Bovine PR 32 supplied safety evidence, not replacement evidence. Contract
  `044`, archived strict spec `114`, and completed card `1099` added atomic
  create-if-absent behavior. The optional-provider and retirement questions in
  this note remain open.

## Next Task

These questions remain open and unscheduled. Card `1099` repaired the retained
surface but supplied no consumer-replacement proof. Do not promote an S3 or
extension lane until that downstream evidence exists.
