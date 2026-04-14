# 005 Optional Distribution Surface Strict Lane

Status: active
Updated: 2026-04-14
Roadmap: `g02.005`

## Context

Effigy now has native distribution commands that replace more of the old
release/distribution script layer. That internal proof is good enough to stop
treating the question as a Rhai-only dogfooding lane.

The next product-shaping problem is optional cross-repo reuse: how Effigy
should expose distribution tooling so other repos can choose to use it without
being forced into Effigy's exact release model.

## Governing Refs

- `docs/architecture/product-guardrails.md`
- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/generation-index.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/005-optional-distribution-surface-contract.md`

## Lane Focus

The active strict lane is:

- define the optional manifest-driven distribution boundary
- separate reusable distribution primitives from Effigy-self-hosting policy
- document the distribution surface as a product front door
- implement only the minimal foundation needed for cross-repo adoption

## Current Posture

`strict-ready`

The Rhai lane is paused on a clean internal boundary. Native distribution
cutover is shipped strongly enough that the next valid move is optional
distribution productization, not more scripting churn.

The first manifest-driven foundation is now shipped for package identity,
preflight tasks, and metadata requirements. The next valid move is an explicit
decision on whether to widen internal command coverage or prove the surface in
one consumer repo.

That widening batch is now shipped too:

- publish identity can be manifest-driven
- summary identity can be manifest-driven
- closeout defaults can be manifest-driven

That decision is now settled: the widened surface is honest enough for one
bounded consumer proof, provided the proof stays focused on optional manifest
adoption rather than claiming every distribution channel shape is already
fully generic.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the distribution question broadens, stop and ask whether the priority is:

- reusable built-ins
- manifest contract design
- or documentation/adoption guidance

Do not guess.

## Exit Condition

This strict lane is complete when Effigy has a documented, optional
distribution surface that other repos can adopt without inheriting Effigy's
hardcoded release policy.

## Next Task

Execute the active `g02.005` ready card to run one bounded consumer-proof
adoption batch for the optional distribution surface.
