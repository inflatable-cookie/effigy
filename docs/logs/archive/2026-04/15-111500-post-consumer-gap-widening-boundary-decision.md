# Post Consumer Gap Widening Boundary Decision

Date: 2026-04-15
Roadmap: `g02.005`
Spec: `docs/specs/005-optional-distribution-surface-strict-lane.md`
Batch Card: `docs/roadmaps/g02/batch-cards/105-decide-post-consumer-gap-widening-boundary.md`

## Decision

Pause `g02.005` on the current optional distribution boundary.

## Why

The lane now has enough real evidence to stop widening:

- `pilot-repo-e` proved cross-repo adoption of the optional manifest surface
- `distribution validate-metadata` now works for manifest-adopting consumers
  without inheriting Effigy's workflow/docs/package gate by default
- `distribution validate-artifacts` now works with consumer-owned publish
  verification toggles
- `distribution generate-closeout` already produces consumer-owned closeout
  evidence cleanly

The remaining open question is narrower and explicitly deferred:

- the full `distribution first-publish` orchestration path still assumes a
  published Cargo install path

That limit no longer undermines the current product claim. The reusable
cross-repo validation and closeout layer is already honest enough to pause.

## Boundary

What is now trustworthy to claim:

- optional manifest-driven package, publish, metadata, preflight, and closeout
  policy
- reusable metadata validation for manifest adopters
- reusable artifact validation
- reusable closeout generation
- optional first-publish verification toggles for Effigy-shaped install probes

What remains explicitly deferred:

- proving the full `distribution first-publish` matrix on a published consumer
- broad non-Cargo publish channel abstraction

## Outcome

`g02.005` is paused. Reopening it now would be churn unless a real
published-consumer need appears and makes the deferred `first-publish`
question concrete again.

## Vision Target Delta

- Primary tags: `CONTRACT`, `OPERATE`, `RELEASE`
- Moved: the optional distribution lane shifted from active widening into a
  paused, trustworthy cross-repo boundary with one explicit deferred limit
- Remaining open: a future published-consumer proof for the fuller
  `distribution first-publish` path, if and when a real repo needs it

## Next Task

`g02.005` is paused. Reopen the lane only when a real published-consumer need
justifies widening the deferred `distribution first-publish` boundary.
