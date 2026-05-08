# 292 Implement Managed Dev Task And Lifecycle Foundation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.013`
Spec: `docs/specs/013-dev-front-door-and-managed-lifecycle-strict-lane.md`

## Objective

Land the first bounded `g02.013` product slice by making a repo-owned managed
dev task able to own named container lifecycle startup and shutdown.

## In Scope

- add manifest/schema support for `tasks.<name>.managed`
- define bounded managed metadata for the first dev-front-door slice
- add explicit concurrent-entry role support for lifecycle ownership
- wire one honest runtime path where a repo-owned managed task can start the
  named container environment and shut it down on owner exit
- cover the new contract in planning/render/help/tests where needed

## Out Of Scope

- embedded shell-tab runtime
- gateway auto-start
- health-gate ready-message UX
- real-project proof work
- turning `effigy dev` into a built-in command

## Acceptance Criteria

- manifests can declare the first bounded `tasks.<name>.managed` contract
- the managed runtime can distinguish lifecycle ownership from generic managed
  processes
- one repo-owned managed task can own named container startup/shutdown on the
  product path
- docs/tests/output surfaces describe the new contract honestly without
  implying later `g02.013` widening is already shipped

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `294` to land the embedded shell-role foundation before widening into
readiness UX or gateway auto-start.
