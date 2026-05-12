# 689 - Refresh Package Map And Crate Boundary Docs

Roadmap: [`../039-artifact-and-crate-boundary-rejustification.md`](../039-artifact-and-crate-boundary-rejustification.md)
Strict lane: [`../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md`](../../../specs/075-artifact-and-crate-boundary-review-strict-lane.md)
Contract: [`../../../contracts/031-artifact-and-crate-boundary-contract.md`](../../../contracts/031-artifact-and-crate-boundary-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Refresh architecture docs so package-map guidance matches current crate
ownership.

## Acceptance

- package-map documents artifact internals and small-crate posture
- historical modularization docs are not treated as current authority

## Outcome

- refreshed `docs/architecture/010-package-map.md`
- added the accepted `effigy-artifacts` module ownership map
- added current small-crate boundary posture and retained-crate rationale
- kept the historical modularization strict lane as paused background, not
  current authority

## Validation

- package-map docs review
- `cargo check --bin effigy`
- `effigy scan god-files --json`

## Next Task

Execute `690` to close the reference-grade cleanup suite.
