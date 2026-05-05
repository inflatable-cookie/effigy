# Post Broad Unlock Confirmation Boundary

Date: 2026-05-05
Roadmap: `g03.027`
Batch Card: `370-decide-post-broad-unlock-confirmation-boundary.md`

## Summary

Optional `init` starter selection is out of scope for the interactive prompt
guardrail lane. The lane should close rather than widen into starter-selection
UX.

## Decision

Next ready card:

- `371-close-interactive-cli-prompt-expansion-lane.md`

Reason:

- prompt guardrail exit conditions are met
- `effigy init` already has deterministic default behavior and `--list`
- starter selection is convenience UX, not a destructive or missing-input
  guardrail

## Vision Target Delta

Primary tags: `CONTRACT`, `OPERATE`, `MAINT`

Baseline: the lane still listed optional `init` starter selection as a possible
follow-up after broad `unlock`.

Current state: optional `init` starter selection is deferred out of this lane,
and `g03.027` can close.

Remaining open: close the roadmap and strict-lane front doors.
