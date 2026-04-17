# 106 Decide Colima Container V1 Contract

Status: complete
Updated: 2026-04-15
Roadmap: `g02.006`
Spec: `docs/specs/archive/006-colima-container-environment-strict-lane.md`

## Objective

Settle the v1 product contract for `effigy container` before implementation
starts.

## In Scope

- command grammar for named and default containers
- manifest registry shape for container environments
- attached owner-session lifecycle rules
- task integration boundary so repos can opt into `effigy dev` without making
  `dev` globally special
- the host/service integration boundary that execution will be allowed to
  assume in v1

## Out Of Scope

- implementing the container runtime
- broad multi-driver abstraction
- Kubernetes, deployment, or CI container policy

## Acceptance Criteria

- the v1 `effigy container` contract is explicit enough that implementation can
  proceed without inventing semantics
- the attached-session ownership model is explicit
- the host/service integration boundary is named instead of deferred into code
- the lane leaves a bounded execution-ready next step

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute the first implementation batch for the v1 Colima container environment
surface.
