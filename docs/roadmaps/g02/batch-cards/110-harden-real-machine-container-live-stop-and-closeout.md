# 110 Harden Real Machine Container Live Stop And Closeout

Status: archived
Updated: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/archive/006-colima-container-environment-strict-lane.md`

## Objective

Close the remaining real-machine hardening gap in the container lane by making
the `colima nerdctl` live-stop and session-closeout path trustworthy enough to
stop treating it as a deferred edge.

## In Scope

- reproduce the current real-machine live-stop weakness on the
  `colima nerdctl compose` path
- harden the stop and closeout behavior so external stop requests route through
  one reliable container shutdown path
- re-prove the fixed path honestly in `example-site`
- update the lane state based on the real result

## Out Of Scope

- broad container-session redesign
- multi-driver work
- convergence with existing repo multi-process `dev` stacks
- distribution release closure work

## Acceptance Criteria

- the real-machine `colima nerdctl` stop path is no longer a known weak edge
- attached task/container session closeout is trustworthy enough to stop
  carrying a deferred warning
- the lane either closes honestly or leaves one tighter residual card

## Validation

- targeted tests for the stop/closeout path
- one real `example-site` proof update
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Pause `g02.006` on the current v1 container boundary and activate `g02.007`
for distribution release closure work.
