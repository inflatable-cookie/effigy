# 305 Implement v0.3 Release-Prep Alignment

Status: archived
Updated: 2026-04-20
Roadmap: `g02.007`
Spec: `docs/specs/007-distribution-release-and-consumer-rollout-strict-lane.md`

## Objective

Land one bounded `g02.007` slice that realigns release-prep evidence, lane
state, and gate posture around the deliberate `v0.3.0` cut.

## In Scope

- refresh release-lane docs/log state so they stop advertising stale
  `v0.2.14` release-readiness language
- capture the current built-in release-command evidence for the intended
  `v0.3.0` cut, including explicit override usage when needed
- fix or account for the current release-gate blocker so the lane has an
  honest readiness checkpoint
- leave the next operator move explicit while still stopping before any
  irreversible release action

## Out Of Scope

- `release prepare --yes`
- `release execute --yes`
- consumer rollout execution
- unrelated product work beyond whatever is necessary to clear the release gate

## Acceptance Criteria

- the release lane points at `v0.3.0`, not stale patch-release language
- the current blocker or ready state is evidenced from live command output
- the next human-approved operator path is explicit and honest
- the lane remains compliant with the no-irreversible-action rule

## Validation

- `cargo run --bin effigy -- release simulate --version 0.3.0`
- `cargo run --bin effigy -- release status --check-gates`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Result

Landed.

The release-prep lane is now realigned on the deliberate `v0.3.0` target:

- release-proof tests and fixtures are back in sync with the current product
  contracts
- `cargo test` passes on the live worktree
- `cargo run --bin effigy -- release status --check-gates` now reports:
  build, format, metadata, qa, smoke, and test all passing
- the built-in release surface is now honestly checkpointed as
  `Ready to prepare and execute: yes`

This card closes the technical prep-alignment gap without crossing into any
irreversible release action.

## Next Task

Stop in planning.

Wait for explicit human approval before any irreversible release action. The
next operator move, if requested, is:

`cargo run --bin effigy -- release prepare --yes --version 0.3.0 --check-gates`
