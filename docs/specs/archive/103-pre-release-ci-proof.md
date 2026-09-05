# 103 - Pre-Release CI Proof

Roadmap: [`g08.030`](../../roadmaps/g08/030-pre-release-ci-proof.md)
Contract: [`039`](../../contracts/039-pre-release-ci-proof-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-11
Completed: 2026-08-11

## Purpose

Require exact-candidate hosted CI evidence before Effigy's release mutation
path can begin.

## Lane Posture

Posture: `strict-complete`

Completed card:

- [`1077`](../../roadmaps/g08/batch-cards/1077-enforce-pre-release-ci-proof.md)

## Settled Decisions

- CI proof is bound to the clean pushed source `HEAD`, not a recent branch run.
- The normal `ci.yml` board is explicitly dispatched and watched before local
  release previews and gates.
- Effigy's repo manifest enforces the proof through a provider-specific gate.
- The reusable release engine remains provider-neutral.
- No workflow edit is needed because `ci.yml` already exposes
  `workflow_dispatch`.

## Acceptance

- [x] successful exact-SHA evidence passes
- [x] absent, pending, red, cancelled, or different-SHA evidence blocks
- [x] protocol ordering is unambiguous for agents and humans
- [x] local release gates are described as additive proof
- [x] focused implementation and docs validation pass

## Evidence

- [`11-182709-pre-release-ci-proof-closeout.md`](../../logs/archive/2026-08/11-182709-pre-release-ci-proof-closeout.md)

## Next Task

Lane complete. Contract `039` owns the invariant.
