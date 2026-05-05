# 367 - Decide Post Container Data Import Confirmation Boundary

Lane: [`033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md`](../033-interactive-cli-prompt-expansion-and-guardrails-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

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
