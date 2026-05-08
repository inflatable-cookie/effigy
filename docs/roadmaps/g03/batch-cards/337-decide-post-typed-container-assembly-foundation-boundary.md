# 337 Decide Post-Typed Container Assembly Foundation Boundary

Status: archived
Updated: 2026-05-02
Roadmap: `g03.014`
Spec: `docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md`

## Objective

Decide whether `g03.014` needs another bounded assembly slice after the first
 typed assembly foundation, or whether the lane is ready to hand off to the
 workspace/runtime orchestrator split.

## In Scope

- inspect the landed `336` surface against the roadmap and strict-lane target
- identify the highest-signal remaining assembly seam, if one still remains
- decide between:
  - one more bounded assembly batch inside `g03.014`
  - or lane closeout with a clean handoff to `g03.015`
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- starting the workspace/runtime orchestrator split
- broad catalog redesign
- new runtime/container features

## Decision

`g03.014` needs one more bounded assembly slice before handoff.

Reason:

- `336` moved shared-service env injection and generated port publication
  onto the typed generated-compose model
- but generated media mounts and generated host mounts still:
  - parse `assembly.compose_yaml` again
  - rediscover repo-root-attached services ad hoc from YAML
  - mutate `volumes` through caller-local YAML helpers

That remaining seam is still central enough to the container brittleness story
that closing `g03.014` now would be premature.

## Acceptance Criteria

- the next honest boundary after `336` is explicit
- the active strict-lane state matches that decision
- no stale ready card is left behind

## Validation

- `./target/debug/effigy docs check-paths docs/specs/028-container-assembly-model-and-single-pass-compose-emission-strict-lane.md docs/roadmaps/g03/batch-cards/336-implement-typed-container-assembly-foundation.md docs/roadmaps/g03/batch-cards/337-decide-post-typed-container-assembly-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/014-container-assembly-model-and-single-pass-compose-emission.md`

## Next Task

Execute `338`.
