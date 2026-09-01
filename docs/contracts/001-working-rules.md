# 001 Working Rules

Status: active
Updated: 2026-08-30

This contract defines how Effigy executes active roadmap work under the strict
Northstar posture.

## Canonical Surfaces

Execution should anchor on these surfaces in order:

1. `docs/roadmaps/generation-index.md`
2. `docs/roadmaps/README.md`
3. the active generation README
4. the active roadmap milestone
5. `docs/specs/README.md`
6. the active spec and current ready batch card
7. `docs/logs/README.md`

Historical logs and handoffs may preserve useful evidence, but they must not be
the only live queue authority.

## Ready-State Rule

Implementation work should only proceed when a bounded ready batch card exists.

A ready card must make these things explicit:

- owner and seam
- governing roadmap/spec context
- acceptance criteria
- validation expectation
- stop conditions

If there is no ready card, the lane is in planning. Do not improvise execution
from a roadmap summary or old handoff note.

## Continue Rule

In a strict Effigy lane, bare `continue` should resolve through the previous
closeout's `Next Task`.

That `Next Task` should normally point at the current ready batch card. If it
does not, refresh the active surfaces before more execution continues.

## Closeout Rule

When a batch closes:

1. update the batch card
2. update the governing roadmap/spec if status or next-step state changed
3. refresh any front-door or currentness surface that still advertises the
   active lane or ready card
4. write one evidence log with validation actually run
5. leave one explicit `Next Task` in the highest-authority active surface

A completed card must never remain advertised as the current ready card.

## Generation Rollover Rule

Treat roadmap generations as substantial sequencing eras, not tiny buckets.
In a long-running repo, expect roughly 20 to 40 roadmap files in one
generation before rollover is even worth discussing.

Treat rollover as full closeout:

- every roadmap in the old generation must be explicitly closed, paused,
  superseded, or moved to backlog
- the roadmap front doors must reflect that closed state before the next
  generation opens
- stale specs and batch cards from the closing generation must be archived or
  removed from `docs/specs/`

If those closeout conditions are not satisfied, repair the current generation
instead of opening a new one.

## Intent Checkpoint Rule

When planning is needed and the next direction is materially ambiguous, stop
and ask for intent instead of guessing.

## Batch Scope Rule

Keep work bounded to one honest owner at a time.

For Effigy this usually means one of:

- bootstrap/repo acquisition behavior
- release/readiness proof
- consumer-adoption or docs-boundary work
- a focused built-in or manifest capability

Do not bundle unrelated product work, release chores, and docs cleanups into
one vague “polish” lane.

## Next Task

Return to planning for official catalog-pack publication and concrete-asset
cutover under contract `043`. That lane is not ready. No official publication,
release/workflow work, or generation rollover has happened.
