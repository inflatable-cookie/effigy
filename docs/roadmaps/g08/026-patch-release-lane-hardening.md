# g08.026 - Patch Release Lane Hardening

Status: Complete
Depends on: `g08.025`
Contracts: [`001`](../../contracts/001-working-rules.md),
[`release orchestration`](../../guides/051-release-orchestration.md)

## Goal

Make Effigy's `0.9.1` patch-release candidate pass its own gates, settle the
prepared-source drift policy, and produce honest read-only release evidence.

## Vision Alignment

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Target envelope: release gates are stable under parallel tests and every
  execute blocker has an explicit operator contract.
- Vision target delta: the release lane moves from consumer-discovered defects
  plus a flaky self-gate to a repeatable patch-release candidate.

## Landed Foundation

Commit `45b0a385a` repaired four consumer-proven release defects: ignored
prepared state, prepare rollback on gate failure, interactive EOF handling,
and bounded Cargo lockfile synchronization. Those changes remain unreleased
and are part of the `0.9.1` candidate.

## Execution Plan

- [x] card 1067: measure and remove persistent loopback state leakage from the
      parallel unit-test gate
- [x] card 1068: decide and freeze the `--allow-stale` source-drift contract in
      behavior, tests, and operator guidance
- [x] card 1069: run focused, full-gate, and read-only release-status proof;
      close the lane without preparing or executing a release

## Goals

- [x] repeated `cargo test --lib` runs do not consume persistent loopback
      assignments or exhaust the bounded pool
- [x] source drift after prepare has one explicit, documented recovery policy
- [x] the `0.9.1` candidate passes the repository's configured release gates
- [x] downstream lockfile-workaround claims stay deferred until the released
      binary is verified against Signal

## Non-Goals

- no release prepare, execute, tag, push, or publication
- no `.github/workflows/` edits
- no pool widening without evidence that capacity is the defect
- no downstream Signal, Longhorn, or Swallowtail mutation before `0.9.1`

## Acceptance Criteria

- [x] the loopback failure is reproduced and classified by measurement
- [x] the fix has a regression test that fails on the unfixed code
- [x] repeated library tests pass without growing the real user registry
- [x] stale-source behavior and JSON recovery guidance agree
- [x] focused release tests, Clippy, formatting, configured gates, and
      `effigy release status` pass or report an explicit remaining blocker

## Stop Conditions

Stop before any release mutation, workflow edit, gate bypass, destructive
cleanup of the user's existing loopback registry, or downstream repository
change.

## Next Task

Request explicit human authorization before release prepare or any later
release mutation.
