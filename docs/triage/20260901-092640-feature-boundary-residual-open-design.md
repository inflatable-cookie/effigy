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

## Next Task

Card `1095` closed on 2026-09-01, so these questions are revisitable in
planning. They are still open and still unscheduled: do not promote any of them
without new consumer/provider evidence that changes contract `043`.
