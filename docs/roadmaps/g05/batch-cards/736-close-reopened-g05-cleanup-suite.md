# 736 - Close Reopened G05 Cleanup Suite

Roadmap: [`../015-active-docs-reference-refresh-and-g05-closeout.md`](../015-active-docs-reference-refresh-and-g05-closeout.md)
Strict lane: [`../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md`](../../../specs/081-post-release-reference-grade-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Close the reopened `g05` cleanup suite once the queued ownership follow-through
cards land or are deliberately deferred.

## Completed

- Closed the reopened `g05` cleanup suite after cards `722` through `735` landed.
- Refreshed roadmap and spec front doors so no stale active lane or ready card
  remains advertised.
- Left unrelated residual blockers documented in lane state and logs instead of
  pretending they were solved inside this suite.

## Residual Blockers

- `cargo test -p effigy-cli` still fails on
  `header::tests::render_cli_header_width_grows_to_fit_long_version`
- `cargo test -p effigy-rhai` still fails on the pre-existing first-party
  script policy checks recorded during `730` and `731`

## Validation

- `git diff --check`

## Next Task

No next task after closeout.
