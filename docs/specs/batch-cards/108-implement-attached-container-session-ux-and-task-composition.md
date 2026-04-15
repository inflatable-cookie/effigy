# 108 Implement Attached Container Session UX And Task Composition

Status: complete
Updated: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/006-colima-container-environment-strict-lane.md`

## Objective

Widen the first container foundation into a real attached operator surface that
repos can compose without falling back to raw shell glue.

## In Scope

- add a bounded attached-session UX on top of `effigy container up`
- reuse Effigy's tabbed/session affordances where they materially help
- expose overview, service, log, and shutdown state clearly enough for real use
- add one explicit repo-owned task-composition path that can reference named
  containers without embedding raw compose commands
- prove the widened UX honestly in the same bounded consumer repo

## Out Of Scope

- broad multi-driver work
- daemonized/background container orchestration
- automatic host DNS or service registration
- broad rollout beyond the first consumer repo

## Acceptance Criteria

- attached `effigy container ...` sessions expose more than raw log follow
- one repo-owned task path can compose named container control honestly
- shutdown behavior remains unified across Ctrl+C, owner exit, and explicit stop
- the resulting consumer proof feels meaningfully less shell-shaped

## Validation

- targeted unit/integration tests for attached-session and task-composition work
- one real consumer proof update
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `109-decide-post-container-session-and-task-composition-boundary.md`
to decide whether the widened v1 proof can now pause honestly or still needs
one more bounded hardening batch.
