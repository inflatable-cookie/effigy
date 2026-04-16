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

The release lane is no longer the active strict lane.

The local Linux rehearsal proof and Rhai dispatch hardening are now shipped,
and `115` remains the release-closure batch for when this lane resumes.

The modularization prerequisite is now treated as still open:

- the extracted domain crates are real
- and the remaining TUI shell is still large enough to justify more
  modularization instead of being treated as incidental adapter work

So the current release posture is:

- local Linux rehearsal is real
- release closure is defined
- actual release-readiness and execution resume only after `g02.010` settles
  those remaining shell seams honestly

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

The next move inside this milestone is still `115`, but it is queued again.

## Exit Condition

This milestone is complete when the distribution surface is both released in
Effigy and rolled out across the intended consumer cohort strongly enough that
it is no longer only an Effigy-local product claim.

## Next Task

Keep
[`115-implement-effigy-distribution-release-closure.md`](../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
queued while `g02.010` resolves the remaining demo-browser TUI shell seam.
