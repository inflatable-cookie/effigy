# 367 - Decide Post Container Data Import Confirmation Boundary

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Decide the next bounded prompt seam after `container data import` confirmation
landed.

## Context

The lane has now applied the shared prompt policy to:

- bootstrap existing non-empty destination reuse
- `container data pull-production`
- `container data import`

The lane exit condition still calls out broad `unlock` confirmation. Before
implementation, confirm the exact unlock shapes that are destructive or
broad-impact enough to require a prompt, and keep script-first unlock flows
explicit.

## Decision Questions

- Should the next implementation card move directly into broad `unlock`
  confirmation?
- Which unlock shapes count as broad enough to guard first?
- What explicit automation bypass should the unlock surface use?
- Are there any prerequisites in the unlock parser or runner path that need a
  smaller foundation card first?

## Exit Condition

This card is complete when the next ready card is either:

- a bounded implementation card for broad `unlock` confirmation, or
- a smaller prerequisite card with the reason documented.

## Non-Goals

- implementing `unlock` confirmation
- reopening container data prompt behavior
- adding `init` starter selection

## Decision

Do not move directly to the final `unlock` prompt implementation yet.

The next card should first promote the prompt policy out of runner-only code so
the built-in `unlock` implementation can use the same decision rules without
duplicating them.

Reason:

- `unlock` is owned by `crates/effigy-builtin`
- the current shared prompt policy is `src/runner/prompt_policy.rs`
- `effigy-builtin` cannot depend on the runner crate
- duplicating prompt rules inside the built-in layer would break the lane's
  single-policy intent

After that foundation lands, `unlock` confirmation should guard these broad
shapes first:

- `effigy unlock --all`
- `effigy unlock workspace`
- any `effigy unlock ...` invocation with more than one explicit scope
- any explicit `shared:<name>` scope, because shared locks can affect multiple
  tasks or profiles

Single `task:<name>` and single `profile:<task>/<profile>` unlocks should stay
unprompted for now. They are precise recovery actions already recommended by
lock-conflict diagnostics.

The automation bypass should be `--yes`, matching the container data prompt
surfaces.

## Next Card

[`368-promote-prompt-policy-for-builtin-unlock.md`](./368-promote-prompt-policy-for-builtin-unlock.md)
is the next prerequisite card.

## Next Task

Execute `368-promote-prompt-policy-for-builtin-unlock.md`.
