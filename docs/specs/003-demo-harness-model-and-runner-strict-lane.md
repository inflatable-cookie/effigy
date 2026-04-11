# 003 Demo Harness Model And Runner Strict Lane

Status: active
Updated: 2026-04-11
Roadmap: `g02.003`

## Context

Manifest composition is now real product surface rather than planning-only
doctrine. That means the next active product question can move to demo proof:
how Effigy should model verification demos, discovery, receipts, coverage, and
browser-facing runner semantics before any UI or project-local harness grows
further.

This spec wraps `g02.003` in the strict execution grammar so the demo model is
designed deliberately before implementation starts.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md`

## Lane Focus

The active strict lane is:

- define the first-class demo object model
- define runner responsibilities separate from UI responsibilities
- keep browser and coverage requirements explicit
- defer implementation until the model and boundaries are coherent

This lane does not start TUI implementation, desktop-client decisions, or repo
migration work.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the demo-harness design proves broader than one bounded batch, stop and ask
whether the priority is:

- demo object model and metadata
- runner lifecycle and artifact semantics
- coverage/gap reporting and browser expectations

Do not guess.

## Exit Condition

This strict lane is complete when `g02.003` no longer relies on loose design
intuition alone and the first implementation-planning step is explicit from the
front doors.

## Next Task

Execute the active `g02.003` coverage/gap ready card next, then either
leave another explicit ready card or return the lane to a real intent
checkpoint.
