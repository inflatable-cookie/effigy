# 294 Implement Managed Dev Shell Role Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Land the next bounded `g02.013` slice by making `role = "shell"` open an
interactive terminal session inside the managed task's target container.

## In Scope

- add explicit `role = "shell"` support for managed concurrent entries
- reuse the shipped `effigy container shell` path against the task-owned
  workspace container binding
- make the shell role honest in both plan rendering and runtime wiring
- cover the bounded shell-role contract in schema/help/tests where needed

## Out Of Scope

- health-gate ready-message UX
- gateway auto-start
- a broader multi-service terminal abstraction beyond the primary-service shell
- real-project proof work

## Acceptance Criteria

- manifests can declare one or more bounded shell-role entries on a managed
  dev task
- a shell-role entry opens the primary-service container shell on the product
  path instead of a generic host shell
- lifecycle and shell roles coexist honestly in one managed task without
  pretending later `g02.013` widening is already shipped
- docs/tests/output surfaces describe the bounded shell-role behavior clearly

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Result

Managed dev tasks can now declare bounded `role = "shell"` entries that open
the shipped primary-service container shell on the managed runtime product
path.

## Next Task

Execute `295` to decide whether readiness UX or gateway auto-start is the next
bounded `g02.013` move.
