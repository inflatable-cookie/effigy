# 298 Implement Managed Dev Gateway Auto-Start Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Land the next bounded `g02.013` slice by making a managed dev task able to
auto-start the shipped gateway path when the task-owned container session
declares DNS ownership.

## In Scope

- add bounded managed-task gateway metadata under `tasks.<name>.managed`
- let a managed dev task start the shipped gateway path against the task-owned
  container session when DNS is configured
- project the gateway auto-start contract through plan/docs/schema/tests where
  needed
- keep shutdown ownership honest relative to the existing managed lifecycle and
  runner cleanup path

## Out Of Scope

- broader multi-project gateway orchestration
- real-project proof work
- lane closeout

## Acceptance Criteria

- manifests can declare bounded gateway auto-start metadata on a managed dev
  task
- the managed dev product path can trigger the shipped gateway startup path
  when the task-owned container session has DNS ownership
- docs/tests/output surfaces describe the bounded gateway behavior clearly
- the batch does not pretend the final real-project proof is already shipped

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Result

Managed dev tasks can now declare `managed.gateway = true`, render that
contract in plan/schema/docs output, and trigger the shipped `effigy gateway
up` path before the lifecycle-owned managed runtime starts.

## Next Task

Execute `299` to decide whether the final `g02.013` move is real-project proof
or direct lane closeout.
