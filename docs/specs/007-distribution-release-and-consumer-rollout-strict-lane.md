# 007 Distribution Release And Consumer Rollout Strict Lane

Status: active
Updated: 2026-04-17
Roadmap: `g02.007`

## Context

The container lane is paused on a trustworthy v1 boundary.

The shipped distribution surface now has:

- local Linux rehearsal on this machine
- in-process Rhai dispatch for the rehearsal path
- one explicit release-closure card

The modularization detour is now closed cleanly, but release execution is no
longer the next product move.

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

`active`

`g02.010` is complete on a trustworthy boundary.

The earlier release pause was sequencing-only. This thread has now resolved the
remaining queue posture strongly enough to return to release prep:

- `g02.002` is complete
- `g02.018` is rehomed into research intake rather than staying in the active
  roadmap queue
- `g02.008` and `g02.009` remain planned rollout lanes, but they are not part
  of the current release-prep batch

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

That hardening detour is now fully closed:

- `cargo run --bin effigy -- qa:ci` passes
- `cargo run --bin effigy -- release simulate` now reports
  `Ready to prepare and execute: yes`
- the suggested release is `v0.2.14`

The next move is no longer another hardening batch.

Release execution is technically ready and `115` is complete.

The remaining gate is now deliberate release execution discipline, not product
sequencing.

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

Return to `115` and the release protocol surfaces for deliberate `v0.3`
release prep.

Stop before any irreversible release action unless explicitly requested.
