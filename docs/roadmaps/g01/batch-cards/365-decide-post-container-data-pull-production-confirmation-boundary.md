# 365 - Decide Post Container Data Pull-Production Confirmation Boundary

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05

## Goal

Decide whether the prompt lane should widen next to `container data import` or
close the container/data subset and move to broad `unlock` confirmation.

## Decision Questions

- did `pull-production` expose any parser or prompt-policy shape that should be
  repaired before another container/data action?
- is `container data import` broad or overwrite-prone enough to justify the
  next prompt seam now?
- should the lane skip import for now and move to broad `unlock` because
  import already requires explicit volume and archive inputs?

## Exit Condition

Close this card only when the next live prompt slice is explicit and bounded.

## Decision

Completed: 2026-05-05

Widen next to `container data import`.

`container data import` already requires an explicit volume and archive path,
but it can still overwrite local generated-compose data. It is also named in
the lane exit condition before broad `unlock`, so skipping it would leave the
container/data subset half-finished.

The next implementation should mirror the `pull-production` shape:

- add `--yes`
- prompt only in eligible TTY flows
- fail clearly for `--json` and non-TTY when `--yes` is absent
- default to no
- show the container, volume, and archive path

## Next Card

- [`366-implement-container-data-import-confirmation.md`](./366-implement-container-data-import-confirmation.md)

## Next Task

Execute `366-implement-container-data-import-confirmation.md`.
