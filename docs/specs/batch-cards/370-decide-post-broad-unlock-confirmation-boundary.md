# 370 - Decide Post Broad Unlock Confirmation Boundary

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Decide whether the interactive prompt lane should still add optional `init`
starter selection or close after broad `unlock` confirmation.

## Context

The lane exit condition now has:

- one shared prompt policy
- bootstrap existing-path reuse confirmation
- `container data pull-production` confirmation
- `container data import` confirmation
- broad `unlock` confirmation
- targeted proof for non-interactive suppression

The original scope listed optional `init` starter selection as a possible final
prompt. That is a convenience prompt, not a destructive guardrail, so it should
not be implemented automatically unless the value still justifies keeping the
lane open.

## Decision Questions

- Should optional `init` starter selection remain in this lane?
- If yes, is it a bounded prompt or a separate UX lane?
- If no, what durable docs or contract updates are needed before closing
  `g03.027`?

## Exit Condition

This card is complete when the next ready card is either:

- an implementation card for optional `init` starter selection, or
- a closeout card for `g03.027`.

## Non-Goals

- implementing `init` starter selection
- changing prompt policy
- reopening bootstrap, container data, or unlock prompt behavior

## Decision

Do not add optional `init` starter selection in this lane.

Reason:

- the lane is now complete against its guardrail exit condition
- `init` starter selection is convenience UX, not destructive or missing-input
  safety
- `effigy init` already has deterministic default behavior and `--list`
- adding selection would widen the lane from prompt guardrails into starter UX

If `init` selection becomes valuable later, open it as a separate UX lane with
starter catalog copy, selection rendering, and no-pressure default behavior as
first-class scope.

## Next Card

[`371-close-interactive-cli-prompt-expansion-lane.md`](./371-close-interactive-cli-prompt-expansion-lane.md)
closes `g03.027`.

## Next Task

Execute `371-close-interactive-cli-prompt-expansion-lane.md`.
