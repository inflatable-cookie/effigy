# 326 Decide Post-Interactive-Ownership Foundation Boundary

Status: archived
Updated: 2026-05-01
Roadmap: `g03.010`
Spec: `docs/specs/023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md`

## Objective

Decide the next honest widening seam after the first shared interactive
ownership foundation.

## In Scope

- audit what `325` now covers:
  - direct `effigy workspace`
  - seeded task shells
  - overlapping `stay_in_shell` / managed seeded cleanup semantics
- inspect what still sits outside the shared ownership helper:
  - attached `container up --attach`
  - any remaining adopted-runtime cleanup branches
  - session-readiness completion that still lives in caller-local code
- decide whether the next batch should:
  - widen interactive ownership into attached operator sessions
  - or stop and hand off to `g03.011`

## Out Of Scope

- implementing the next widening batch itself
- embedded-runner convergence
- new regression matrix work

## Acceptance Criteria

- the post-`325` gap is explicit
- the lane outcome is explicit enough to either widen cleanly or hand off
  without reopening ownership policy debate

## Validation

- docs-only: `./target/debug/effigy docs check-paths docs/specs/023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md docs/roadmaps/g03/batch-cards/325-implement-interactive-ownership-classification-foundation.md docs/roadmaps/g03/batch-cards/326-decide-post-interactive-ownership-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/010-interactive-session-ownership-and-lifecycle-convergence.md`

## Next Task

Promote `g03.011`.
