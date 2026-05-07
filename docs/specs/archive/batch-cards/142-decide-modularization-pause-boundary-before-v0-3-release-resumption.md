# 142 Decide Modularization Pause Boundary Before v0.3 Release Resumption

Status: complete
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether `g02.010` has now reached a trustworthy pause boundary for
`v0.3`, or whether one more modularization batch is still required before the
queued release lane can resume.

## In Scope

- assess the remaining major runner-local clusters after the doctor widening
- distinguish honest shell/orchestration work from still-reusable domain debt
- decide whether `g02.010` can pause and hand control back to `g02.007`

## Out Of Scope

- executing the release lane in the same batch
- adding a new extraction batch unless the decision explicitly requires it
- vault-provider rollout work

## Acceptance Criteria

- the current modularization boundary is classified honestly
- the next lane move is explicit
- roadmap/spec currentness stays trustworthy

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

`g02.010` can now pause.

The remaining large runner-local command files are still heavy, but they are
now predominantly shell, render, TUI, git, and process orchestration over
real extracted domain crates:

- `effigy-core`
- `effigy-tasks`
- `effigy-manifest`
- `effigy-containers`
- `effigy-distribution`
- `effigy-release`
- `effigy-rhai`
- `effigy-demo`
- `effigy-docs-policy`
- `effigy-env`
- `effigy-doctor`

That is enough to treat the pre-`v0.3` modularization bar as met. The release
lane can resume without pretending the shell itself is already small.

## Next Task

Resume `g02.007` and execute
[`115-implement-effigy-distribution-release-closure.md`](./115-implement-effigy-distribution-release-closure.md).
