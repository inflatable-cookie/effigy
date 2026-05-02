# 358 Implement Runtime/Container Proof-Matrix Foundation

Status: complete
Updated: 2026-05-02
Roadmap: `g03.018`
Spec: `docs/specs/032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md`

## Objective

Land the first bounded proof matrix for the runtime/container core instead of
 claiming v1 confidence from architecture cleanup alone.

## In Scope

- define and automate the first high-signal runtime/container proof matrix for:
  - bootstrap setup tasks versus bootstrap handoff shell ownership
  - lease-refresh versus no-lease activation paths
  - direct workspace versus seeded workspace session cleanup
  - typed gateway/runtime reconciliation parity on reused runtimes
- add or tighten drift guards where the proof matrix would otherwise rely on
  reading code by hand
- update the active docs surfaces so the proof lane is explicit

## Out Of Scope

- new container features
- broad performance benchmarking
- provider deployment export work
- speculative QA expansion outside the runtime/container hardening seam

## Acceptance Criteria

- the first bounded proof matrix exists as executable validation, not prose
- the chosen scenarios directly cover the historical brittle seams behind the
  recent hardening work
- the lane is in a position for an honest boundary decision

## Validation

- targeted runtime/container proof tests
- `./target/debug/effigy docs check-paths docs/specs/032-v1-runtime-hardening-proof-and-stress-matrix-strict-lane.md docs/specs/batch-cards/358-implement-runtime-container-proof-matrix-foundation.md docs/specs/batch-cards/359-decide-post-proof-matrix-foundation-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/018-v1-runtime-hardening-proof-and-stress-matrix.md`

## Next Task

Closed. Execute `359`.
