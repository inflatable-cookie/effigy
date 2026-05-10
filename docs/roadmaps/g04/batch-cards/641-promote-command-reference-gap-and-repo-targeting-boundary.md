# 641 - Promote Command Reference Gap and Repo Targeting Boundary

Roadmap: [`../024-command-reference-completeness-and-flag-consistency.md`](../024-command-reference-completeness-and-flag-consistency.md)
Strict lane: [`../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md`](../../../specs/067-command-reference-completeness-and-flag-consistency-strict-lane.md)
Contract: [`../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md`](../../../contracts/022-command-reference-completeness-and-flag-consistency-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Purpose

Lock the implementation boundary for `g04.024` before parser, runner, or guide
updates start.

## Scope

- confirm the exact missing command/flag set the lane is allowed to fix
- lock the `version` documentation rule
- lock the bounded `--repo` widening for `changelog` and `bundle`
- keep the lane out of bundle-source, container-behavior, and changelog-behavior drift

## Acceptance

- contract `022` covers the full bounded command/flag gap set
- `067` points at this card as the current ready step
- `024` and the front doors all reflect the active lane and next step
