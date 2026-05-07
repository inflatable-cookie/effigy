# 001 - Runtime Architecture Sanity Audit And Generation Rollover

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Land the runtime architecture sanity audit and open `g04` as the next clean
roadmap generation.

## Scope

- add `docs/architecture/022-runtime-architecture-sanity-audit.md`
- create the `g04` roadmap front door
- update roadmap generation front doors
- create strict lane `043`
- create first batch card `431`
- create the next ready card for `g04.002`
- no implementation code changes

## Non-Goals

- no crate extraction
- no runtime behavior changes
- no release work
- no `.github/workflows/` edits

## Acceptance Criteria

- audit lists critical path hotspots
- audit lists direct-call drift inventory
- `g03` is visibly closed
- `g04` is current generation
- first strict lane points to the next ready card
- no stale `g03` ready card remains

## Closeout

- `022-runtime-architecture-sanity-audit.md` exists.
- `g04/README.md` exists and lists the architecture simplification queue.
- roadmap generation index marks `g04` current.
- strict lane `043` is active.
- card `431` is complete.
- card `432` is ready for `g04.002`.

## Next Task

Start card
[`432-scaffold-execution-pipeline-ownership-lane.md`](../../specs/batch-cards/432-scaffold-execution-pipeline-ownership-lane.md).
