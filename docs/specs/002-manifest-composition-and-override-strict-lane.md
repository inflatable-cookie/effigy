# 002 Manifest Composition And Override Strict Lane

Status: active
Updated: 2026-04-11
Roadmap: `g02.002`

## Context

Bootstrap is now complete as a released and live-pilot-validated lane. The next
active product question is broader and more foundational: how Effigy should
compose split manifest config without letting features invent their own external
file-loading semantics.

This spec wraps `g02.002` in the strict execution grammar so the composition
contract is designed deliberately before the demo harness or other features
start depending on it.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/002-manifest-composition-and-override-contract.md`

## Lane Focus

The active strict lane is:

- define one feature-agnostic manifest composition model
- define explicit override behavior rather than silent merge folklore
- keep the decision bounded enough that `g02.003` can plan against it without
  implementation beginning early

This lane does not start demo implementation, desktop-client decisions, or
general “split everything into files” cleanup.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the composition and override tradeoffs prove broader than one bounded batch,
stop and ask whether the priority is:

- root composition syntax and merge behavior
- override semantics and conflict handling
- explainability/tooling shape before config syntax

Do not guess.

## Exit Condition

This strict lane is complete when `g02.002` no longer relies on implied design
preference alone and the next implementation-planning step is explicit from the
front doors.

## Next Task

Execute the active `g02.002` implementation-ready card next, then either leave
another explicit ready card or return the lane to a real intent checkpoint.
