# Feature Boundary Open Design

Status: open
Created: 2026-08-31
Owner: orchestrator
Architecture: [`026`](../architecture/026-feature-placement-and-command-surface.md)
Contract: [`043`](../contracts/043-feature-placement-and-surface-migration-contract.md)

## Purpose

Preserve the unresolved design choices left after promoting the feature-
placement audit. Settled ownership and migration rules live in architecture
`026` and contract `043`; do not reopen them from this note.

## Open Questions

- What exact namespace names and grammar best express daily work, local
  runtime, repository intelligence, delivery/state, and extensions?
- Which direct aliases should remain permanent? No alias removal is approved.
- What extension transport should optional runtime/provider code use?
- What is the minimum base Rhai surface after provider-specific helpers move?
- How should the default catalog pack be installed and updated while remaining
  automatic, offline-capable, inspectable, and no harder to use than today?
- What exact consumer evidence will show the
  `bovine-accelerator-desktop` media-upload replacement is live and the
  `bovine-accelerator` Rhai storage dependency can retire?
- What evidence threshold should justify a future provider implementation in
  mandatory core?

## Settled Constraints

- semantic ownership, not universality or façade exposure, decides core;
- grouping is approved, but existing direct routes remain stable initially;
- repository intelligence remains core;
- catalog externalization cannot add operator ceremony;
- release transaction safety remains core while Effigy-specific distribution
  recipes move outward;
- S3 remains until the named consumer migration is proved.

## Next Task

After card `1090` settles, use decision prototypes to resolve namespace grammar
and catalog-pack acquisition, then compile separate migration lanes. Keep S3
out of the implementation queue until its consumer gate is met.
