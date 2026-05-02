# 339 Decide Post-Typed Mount Attachment Boundary

Status: complete
Updated: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`

## Objective

Decide whether `g03.014` can close after the typed mount-attachment slice, or
whether another bounded assembly seam still remains.

## Decision

Close `g03.014` and hand off to `g03.015`.

Why:

- the main generated-compose policy seams are now on typed ownership:
  - shared-service env injection
  - generated port publication
  - generated media mount attachment
  - generated host mount attachment
- the remaining YAML rewrite hotspots are in `workspace.rs`
- those are workspace/runtime orchestration seams, not container assembly core

## In Scope

- inspect the landed `338` surface against the roadmap and strict-lane target
- decide whether the lane is now complete enough to hand off to `g03.015`
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- starting the workspace/runtime orchestrator split
- broad catalog redesign
- new runtime/container features

## Acceptance Criteria

- the next honest boundary after `338` is explicit
- no stale ready card is left behind
- the strict-lane state matches reality

## Validation

- `./target/debug/effigy docs check-paths docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md docs/specs/batch-cards/338-implement-typed-mount-attachment-assembly-slice.md docs/specs/batch-cards/339-decide-post-typed-mount-attachment-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md`

## Next Task

Promote `g03.015`.
