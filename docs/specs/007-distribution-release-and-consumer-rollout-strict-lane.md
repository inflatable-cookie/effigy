# 007 Distribution Release And Consumer Rollout Strict Lane

Status: queued
Updated: 2026-04-15
Roadmap: `g02.007`

## Context

The container lane is paused on a trustworthy v1 boundary.

The modularization lane is active again because the user does not consider the
remaining TUI shell weight release-ready yet.

The shipped distribution surface now has:

- local Linux rehearsal on this machine
- in-process Rhai dispatch for the rehearsal path
- one explicit release-closure card

That prerequisite is no longer treated as met strongly enough, so release
closure is queued again behind `g02.010`.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/README.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/007-distribution-release-and-consumer-rollout.md`
- `docs/guides/049-ci-binary-distribution-and-release-protocol.md`
- `docs/guides/051-release-orchestration.md`

## Lane Focus

This strict lane remains responsible for:

- close the shipped optional distribution surface through release work
- retain the local Linux build proof path for pre-release prep
- follow the release with deliberate consumer rollout work

## Current Posture

`strict-queued`

`g02.006` is paused. `g02.010` is active again. `g02.007` is queued again.

The release-prep hardening chain is real:

- Effigy has a named `linux-release` container bound to Ubuntu 22.04
- `release:linux:env` exposes that environment as a repo-owned attached task
- `release:linux:rehearse` proves local Linux build, smoke, and GLIBC
  validation through the shipped container surface
- `effigy container shell --command <CMD>` now runs through `sh -lc`
- Rhai scripts can call the running Effigy process through
  `run_effigy(...)` and `run_effigy_json(...)`
- the first typed container helpers now exist:
  `container_up(...)`, `container_down(...)`, `container_shell(...)`
- `release:linux:rehearse` no longer re-enters through
  `cargo run --bin effigy`

That closes the release-prep hardening detour, but release closure is queued
again until the remaining demo-browser TUI modularization question is settled
honestly.

## Batch Model

- planning stays in this spec plus the roadmap
- execution proceeds only from a ready card
- each ready card must leave the lane either:
  - with another explicit ready card
  - or back in planning with an intent checkpoint

## Intent Checkpoint

If the release question broadens, stop and ask whether the priority is:

- local release proof and rehearsal
- Effigy release execution
- or consumer rollout sequencing

Do not guess.

## Exit Condition

This strict lane is complete when the shipped distribution surface is released
in Effigy with an honest supported boundary and rolled out across the intended
consumer cohort strongly enough that it no longer rests only on Effigy-local
claims.

## Next Task

Keep
[`115-implement-effigy-distribution-release-closure.md`](./batch-cards/115-implement-effigy-distribution-release-closure.md)
queued while `g02.010` resolves the remaining demo-browser TUI shell seam.
