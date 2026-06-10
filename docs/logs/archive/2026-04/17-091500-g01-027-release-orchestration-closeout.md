# 2026-04-17 Release Orchestration Closeout

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Moved from `g01.027 still treated as a live pre-v0.3 blocker` to
  `g01.027 closed on shipped release surface, production proof, and wrapper retirement evidence`
- Remaining open: `g02.010` final `/src` cleanup and reconciliation

## Summary

`g01.027` was stale-open.

The release orchestration surface is already shipped and proven:

- built-in `release status`, `gates`, `simulate`, `prepare`, `resume`,
  `execute`, and `verify-install`
- production built-in release proof for `v0.2.5`
- workflow cutover to `effigy changelog extract`
- legacy release compatibility wrappers retired later once the cutover settled

The roadmap stayed open because the planning state still reflected the earlier
comparison-window language after the repo had already moved past it.

## Evidence

- first production built-in release:
  - `docs/logs/archive/2026-03/12-131500-release-checkpoint-v0-2-5.md`
- hosted workflow cutover using built-in changelog extraction:
  - `docs/logs/archive/2026-03/11-183500-release-workflow-cutover-hosted-validation.md`
- wrapper retirement and native release cutover:
  - `docs/logs/archive/2026-04/15-013500-release-wrapper-retirement-and-native-cutover.md`
- current operator protocol and command contract:
  - `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
  - `docs/guides/051-release-orchestration.md`

## Outcome

`g01.027` is now marked `Complete`.

The remaining release work is normal operator use of the shipped surface once
`g02.010` is out of the way. It is no longer roadmap implementation debt.

The remaining pre-`v0.3` blocker set is now one item:

1. `g02.010` final `/src` cleanup and reconciliation

