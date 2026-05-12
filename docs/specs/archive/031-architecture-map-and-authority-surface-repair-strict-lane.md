# 031 Architecture Map And Authority Surface Repair Strict Lane

Status: complete
Updated: 2026-05-02
Roadmap: `g03.017`

## Context

`g03.016` closed strongly enough to stop treating runtime/container failure
 shape as the main hardening seam.

The next honest weakness is architecture authority:

- some architecture/package maps no longer describe the real codebase
- current ownership for runtime/container seams is now clearer in code than in
  docs
- future planning will drift again if the authority surfaces stay stale

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g03/017-architecture-map-and-authority-surface-repair.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- repairing stale architecture and package-map authority surfaces
- making current ownership explicit for:
  - runner orchestration
  - runtime/session context
  - container assembly
  - workspace handoff and provisioning
  - runtime/container error families
- shrinking or replacing authority docs that cannot be kept current

This lane does not own:

- reopening completed runtime/container code work
- new product behavior
- broad docs cleanup outside architecture authority
- final v1 proof-matrix work

## Current Posture

`strict-active`

## Continuation Chain

1. `356` — inventory stale architecture authority surfaces and repair the front
   doors
2. `357` — decide whether the lane needs one more bounded repair slice or can
   hand off to `g03.018`

## Exit Condition

This strict lane is complete when:

- the architecture front doors match the actual current runtime/container
  structure
- no major live subsystem is primarily described by stale historical docs
- the next proof lane can rely on the repaired authority surfaces as real
  ownership references

## Next Task

Closed. Promote `g03.018`.
