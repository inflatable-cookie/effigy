# 335 Decide Post-Typed Activation And Session Context Foundation Boundary

Status: archived
Updated: 2026-05-02
Roadmap: `g03.013`
Spec: `docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md`

## Objective

Decide whether `g03.013` needs one more bounded runtime-ownership slice after
 the typed activation/session context foundation, or whether the lane is ready
 to hand off to the typed container assembly model.

## In Scope

- inspect the landed `334` surface against the roadmap and strict-lane target
- identify the highest-signal remaining runtime ownership seam, if one still
  remains
- decide between:
  - one more bounded runtime/session ownership batch inside `g03.013`
  - or lane closeout with a clean handoff to `g03.014`
- refresh the active strict-lane and front-door surfaces to match that
  decision

## Out Of Scope

- starting the typed container assembly refactor
- broad workspace/runtime file splitting
- new runtime/container features

## Acceptance Criteria

- the next honest boundary after `334` is explicit
- the active strict-lane state matches that decision
- no stale ready card is left behind

## Validation

- `./target/debug/effigy docs check-paths docs/specs/027-runtime-session-context-and-runtime-ownership-hardening-strict-lane.md docs/roadmaps/g03/batch-cards/334-implement-typed-activation-and-session-context-foundation.md docs/roadmaps/g03/batch-cards/335-decide-post-typed-activation-and-session-context-foundation-boundary.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/013-runtime-session-context-and-runtime-ownership-hardening.md`

## Decision

Close `g03.013` and hand off directly to `g03.014`.

`334` removed the main bootstrap/runtime ownership brittleness the lane set
 out to fix:

- typed lease-refresh policy now governs activation across routed task,
  deferred, and explicit exec paths
- typed public-workspace cleanup override now governs bootstrap handoff
  stop-on-exit behavior
- the main workspace, seeded-shell, bootstrap, and exec seams are covered by
  focused proof

There is still more runtime cleanup worth doing, but it is no longer the
 highest-signal `v1.0` seam. The next honest risk is the YAML-rewrite-heavy
 container assembly core.

## Next Task

Promote `g03.014`.
