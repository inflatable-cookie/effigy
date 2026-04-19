# 007 - Distribution Release And Consumer Rollout

Generation: `g02`

Status: In Progress
Owner: Platform
Created: 2026-04-15
Depends on: 005, 027

## Problem

Effigy's optional distribution surface is now paused on a trustworthy product
boundary, but that work is not yet closed in the ways that matter externally:

- Effigy still needs a release that frames the distribution surface as shipped
- consumer repos still need deliberate rollout where the surface actually helps

## Goal

Close the distribution lane properly by:

- rolling an Effigy release that includes the optional distribution mechanism
- documenting the exact supported boundary honestly
- then adopting the surface across real consumer repos where it fits

## Scope

- one local Linux release-rehearsal environment so pre-release prep can prove
  the Linux build path before CI
- release notes, changelog, and release execution for the shipped boundary
- one honest rollout cohort across consumer repos
- no over-claim that the full published-consumer `first-publish` path is
  universally generic yet

## Current Focus

The release lane is intentionally deferred now.

The local Linux rehearsal proof, Rhai dispatch hardening, modularization, and
release-closure prep are all done. `115` is complete and the release posture is now:

- local Linux rehearsal is real
- release closure is complete
- `qa:ci` passes
- `release simulate` says `Ready to prepare and execute: yes`
- the suggested release is `v0.2.14`

Shipped proof already in place:

- Effigy owns one `linux-release` container on Ubuntu 22.04
- pre-release prep can run `cargo run --bin effigy -- release:linux:rehearse`
  to build the Linux binary, run `smoke:release`, and validate the GLIBC floor
  locally
- manual inspection can use `cargo run --bin effigy -- release:linux:env`

- Rhai has in-process `run_effigy(...)` and `run_effigy_json(...)`
- the first typed container helpers exist for release/container scripting
- the Linux rehearsal proof now runs through the live Effigy runtime instead of
  `cargo run --bin effigy`

The release gate is now broader product readiness, not repo hardening.

The earlier pause was about product sequencing, not missing release prep.

That sequencing checkpoint is now resolved for this thread:

- `g02.002` is closed
- `g02.018` is rehomed into research intake instead of staying in the active
  roadmap queue
- `g02.008` and `g02.009` remain valid rollout work, but they are intentionally
  out of the current `v0.3` release-prep thread

The lane now returns to deliberate release prep for one explicit human-approved
`v0.3` cut.

## Exit Condition

This milestone is complete when the distribution surface is both released in
Effigy and rolled out across the intended consumer cohort strongly enough that
it is no longer only an Effigy-local product claim.

## Next Task

Return to `115` and the release protocol surfaces for deliberate `v0.3`
release prep.

Stop before any irreversible release action unless explicitly requested.
