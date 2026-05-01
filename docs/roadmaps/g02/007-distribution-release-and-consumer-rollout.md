# 007 - Distribution Release And Consumer Rollout

Generation: `g02`

Status: Complete
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

The original release lane is no longer the live queue.

The local Linux rehearsal proof, Rhai dispatch hardening, modularization, and
release-closure prep are all done. `115` is complete and the release posture is now:

- local Linux rehearsal is real
- release closure is complete
- `qa:ci` passes
- `release simulate` says `Ready to prepare and execute: yes`
- the repo is now deliberately targeting `v0.3`, not a `v0.2.14` patch release

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

The lane has now completed that bounded alignment slice:

- the release-prep checkpoint is refreshed through `305`
- `cargo test` passes on the live worktree
- `cargo run --bin effigy -- release status --check-gates` reports all gates
  passing and `Ready to prepare and execute: yes`
- the built-in release flow still defaults to a patch suggestion (`0.2.14`)
  unless the deliberate `0.3.0` target is chosen explicitly

That release execution work is now done:

- `v0.3.0` shipped
- `v0.3.1` shipped

Any remaining consumer-rollout question should be re-sequenced deliberately
instead of pretending this is still an active release-prep lane.

Supported-boundary rule:

- release messaging should frame the distribution surface as strong native
  self-hosting plus reusable validation/evidence primitives
- it should not over-claim the fuller `distribution first-publish` path as
  universally generic while that path remains intentionally bounded

## Exit Condition

This milestone is now complete on the `g02` boundary.

The release work shipped through `v0.3.0` and `v0.3.1`. Any broader
consumer-rollout question is no longer a `g02` release-lane concern.

## Next Task

Leave this roadmap closed.

If broader consumer rollout becomes real again, rehome it into the live
generation instead of resuming the old `v0.3` release lane.
