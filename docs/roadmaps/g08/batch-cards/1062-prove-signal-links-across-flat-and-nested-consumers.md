# 1062 - Prove Signal Links Across Flat And Nested Consumers

Roadmap: [`../023-dependency-link-portfolio-proof-and-closeout.md`](../023-dependency-link-portfolio-proof-and-closeout.md)
Strict lane: [`../../../specs/099-local-dependency-management-strict-lane.md`](../../../specs/099-local-dependency-management-strict-lane.md)
Contract: [`../../../contracts/034-local-dependency-linking-contract.md`](../../../contracts/034-local-dependency-linking-contract.md)

Status: Complete
Owner: Platform
Created: 2026-08-05
Ready after: completed card `1061`

## Purpose

Prove the Cargo link contract against Signal `v0.1.0`, Soundcheck's flat
workspace, and every Signal-consuming workspace in Loophole without touching
the live portfolio worktrees.

## Owner And Seam

The existing `effigy-deps` Cargo plan/apply/status/unlink path is under proof.
This card may add bounded proof harnesses and fix defects exposed by real repo
shapes, but it must not add a parallel portfolio-only implementation.

## Work

- create disposable local clones from the committed `HEAD` of Signal,
  Soundcheck, and Loophole; do not use or mutate their live worktrees
- record the exact Signal packages selected in Soundcheck and in each
  Signal-consuming Loophole workspace
- link Signal with the current local Effigy binary and prove every selected
  crate resolves from the disposable Signal checkout
- make one harmless source edit in the disposable Signal clone and prove a
  bounded consumer rebuild observes it
- inspect `effigy deps status` and `effigy doctor` text/JSON while linked
- unlink, prove resolution returns to the exact `v0.1.0` Git source, and prove
  manifest and lockfile state returns clean
- record commands, package/workspace closure, timings, and any bounded defect
  correction in one portfolio-proof log

## Guardrails

- no writes in `/Users/tom/Dev/projects/{signal,soundcheck,loophole}`
- no network clone requirement; use local committed repository objects
- no consumer manifest migration or committed fixture churn
- no manual Cargo patch edits or Git lockfile restore commands
- no scope expansion into Bun proof or operator-guide authoring

## Acceptance

- [x] Soundcheck resolves its full matching Signal closure locally
- [x] every Loophole workspace that consumes Signal is discovered and linked
- [x] a disposable Signal edit is consumed by a bounded rebuild
- [x] status and doctor agree on local resolution and the expected active-lock
      do-not-commit errors
- [x] unlink restores tagged resolution and clean manifest/lock state
- [x] the proof performs no writes in live portfolio worktrees; live HEAD and
      tracked-diff hashes remain unchanged

## Validation

- disposable-clone link/status/doctor/unlink transcript
- `cargo metadata` and targeted `cargo tree` source assertions
- bounded consumer rebuild before and after the Signal edit
- `git status --short` before/after for live and disposable repos
- focused Effigy regression tests for any defect correction
- `git diff --check`

## Stop Conditions

Stop and replan if proof requires modifying a live portfolio worktree, a
consumer is not yet on the tagged Signal source, the full closure cannot be
identified deterministically, or a failure requires changing the durable
dependency-link contract.

## Next Task

Execute ready Bun portfolio-equivalent proof card `1063`.
