# 003 Demo Harness Model And Runner Strict Lane

Status: complete
Updated: 2026-04-13
Roadmap: `g02.003`

## Context

Manifest composition is now real product surface rather than planning-only
doctrine. That means the next active product question can move to demo proof:
how Effigy should model verification demos, discovery, receipts, coverage, and
browser-facing runner semantics before any UI or project-local harness grows
further.

This spec wraps `g02.003` in the strict execution grammar so the demo model and
runner can keep moving in bounded batches now that registry loading,
inspection, lifecycle control, query polish, the first browser foundation, and
bounded runner-side attempt history are real shipped surface.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/003-demo-harness-model-and-runner-contract.md`

## Lane Focus

The active strict lane is:

- keep the first-class demo object model and runner boundaries coherent as
  implementation starts
- land runner execution in bounded slices on top of the shipped registry and
  inspection foundation
- keep browser and coverage requirements explicit as the first TUI surface
  starts to exist
- defer desktop-client decisions and generic runtime cancellation expansion
  until the runner/runtime surface is honest enough to support them

This lane does not start desktop-client decisions, generic runtime cancellation,
or repo migration work.

## Current Posture

`complete`

`g02.003` is now shipped and released in `v0.2.13`. The lane no longer governs
active work; scripting strategy moved into `g02.004`.

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

This strict lane is complete. The shipped demo/browser surface now lives in the
roadmap and guides rather than an active strict execution lane.

## Next Task

Use `g02.004` as the active strict lane next. `g02.003` is closed and no
longer carries a ready card.
