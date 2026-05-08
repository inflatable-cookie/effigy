# 360 Implement Host-Integration And Shared-Service Proof Slice

Status: archived
Updated: 2026-05-02
Roadmap: `g03.018`
Spec: `docs/specs/032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md`

## Objective

Land one more bounded runtime/container proof slice for the seams `358` did not
 cover: workspace host integration, external mounts, and shared-service
 env/binding behavior.

## In Scope

- add executable proof for representative workspace host-integration paths,
  including at least:
  - shared composer-home ownership behavior
  - full SSH-dir host mount opt-in behavior
- add executable proof for shared-service env/binding behavior, including:
  - host/port env projection from shared-service catalog metadata
  - runtime-facing alias/binding expectations staying stable
- include at least one representative local stack proof that exercises these
  seams together strongly enough to stop treating them as under-proven

## Out Of Scope

- new host-integration features
- broader provider/deploy export work
- performance tuning
- architecture-authority rewrites

## Acceptance Criteria

- the remaining under-proven host-integration and shared-service seams now have
  executable coverage
- the lane is in position for an honest final closeout decision
- the next boundary is explicit

## Validation

- targeted host-integration and shared-service proof tests
- `./target/debug/effigy docs check-paths docs/specs/032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md docs/roadmaps/g03/batch-cards/359-decide-post-proof-matrix-foundation-boundary.md docs/roadmaps/g03/batch-cards/360-implement-host-integration-and-shared-service-proof-slice.md docs/roadmaps/g03/batch-cards/361-decide-post-host-integration-proof-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/018-v1-runtime-hardening-proof-and-stress-matrix.md`

## Next Task

Closed. Execute `361`.
