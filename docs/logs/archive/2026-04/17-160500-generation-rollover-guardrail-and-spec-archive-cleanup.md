# Generation Rollover Guardrail And Spec Archive Cleanup

Date: 2026-04-17
Roadmap: `g02.007`, `g02.010`

## Summary

Effigy's roadmap doctrine now blocks generation rollover until the current
generation is fully closed out and the active specs tree has been purged.

This batch also archived the stale strict-lane artifacts for specs `001`
through `006` so the active `docs/specs/` tree stops carrying obviously closed
history.

## What Changed

- updated the roadmap front doors and working rules to treat rollover as a
  closeout event rather than a convenience reset
- added the healthy-generation scale guardrail: roughly `20` to `40` roadmap
  files before rollover is even worth discussing
- added `docs/specs/archive/` and `docs/roadmaps/g04/batch-cards/` as the
  explicit home for closed or paused strict-lane history
- moved specs `001` through `006` plus their batch cards into that archive
- rewrote `docs/roadmaps/g04/batch-cards/README.md` so it points at the real live
  chain (`245` -> `246`, with `115` complete but deferred) instead of dumping
  the whole historical card corpus into the active front door

## Current State

- active `docs/specs/` is now centered on the live or near-live lanes
- stale early-generation strict-lane debris no longer competes with the live
  queue in the active tree
- Effigy's own docs now say `g02` cannot roll until the roadmap set is closed
  cleanly and stale `g02` specs are purged

## Boundary Call

This does not justify `g03`.

It makes the rollover bar explicit and removes one chunk of stale planning
debris, but `g02` still has live roadmap work and remains the active
generation.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `ROUTE`
- moved from `rollover posture implicit and active specs tree carrying closed early lanes`
  to `rollover posture explicit and early closed strict lanes archived out of the active tree`
- remains open: finish shrinking the active `g02` roadmap/spec surface until a
  true full-generation closeout is possible

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Finish the live `g02.010` card chain (`245` then `246`), then return to the
deferred release decision from `115`.
