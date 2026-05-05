# 363 - Decide Post Bootstrap Path-Reuse Prompt Boundary

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05

## Goal

Decide whether the prompt policy plus bootstrap path-reuse slice is strong
enough to widen directly into destructive container/data actions.

## Decision Questions

- is the prompt policy explicit enough now, or is one more shared helper slice
  still needed before widening?
- did bootstrap path reuse expose any interaction-model drift that should be
  fixed before touching container/data surfaces?
- is `container data pull-production` clearly the next highest-value prompt
  seam?

## Exit Condition

Close this card only when the next live prompt slice is explicit, bounded, and
sequenced.

## Decision

Completed: 2026-05-05

Widen directly into `container data pull-production`.

The prompt policy from card `362` is strong enough for the next destructive
container/data seam:

- prompt eligibility is explicit and tested
- `--json`, `--plan`, and non-TTY behavior are already script-safe
- bootstrap path reuse did not expose a need for a helper-only card

The next implementation should add a dedicated non-interactive bypass instead
of reusing bootstrap's `--no-prompt`. For destructive container/data actions,
`--yes` is clearer operator language.

## Next Card

- [`364-implement-container-data-pull-production-confirmation.md`](./364-implement-container-data-pull-production-confirmation.md)

## Next Task

Execute `364-implement-container-data-pull-production-confirmation.md`.

Decide whether to widen directly into `container data pull-production` prompts
or first harden the shared helper surface exposed by card `362`.
