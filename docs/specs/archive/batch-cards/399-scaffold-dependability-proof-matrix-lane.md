# 399 - Scaffold Dependability Proof Matrix Lane

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Turn `g03.034` into an executable proof lane with a bounded first fixture card.

## Scope

- open strict lane `040`
- define the proof matrix rows and fixtures
- inventory existing tests that can host the new proof cases
- choose the first implementation card
- no fixture implementation yet

## Exit Condition

This card is complete when the proof matrix has a first implementation card
with a narrow write set.

## Matrix

| Row | Shape | Proof |
| --- | --- | --- |
| DecodeLabs mysql seed | Rhai script imports a repo-owned SQL file through a container-targeted execution request | `exec::run(..., #{ run_in: "container", service: "db", stdin_file: ... })` keeps repo-relative stdin path and routes to container exec |
| DecodeLabs bundle shape | bundle-local seed paths are resolved from target repo, not invocation cwd | follow-up fixture in runner script/bootstrap tests |
| Underlay generated compose | generated compose path and external mount mapping stay stable | follow-up fixture in `effigy-containers` policy/compose tests |
| Bootstrap target repo | bootstrap task execution keeps target repo root | follow-up bootstrap CLI/unit proof |
| Inside-container re-entry | captured runtime context prevents host/container drift | follow-up execution/context proof |
| Manager reports | operation reports include backend, repo, action, cleanup | follow-up manager/container command proof |

## Inventory

- `crates/effigy-rhai/src/tests.rs` already proves `runtime::context()` and
  `exec::run(...)` surfaces with host and container callbacks.
- `crates/effigy-execution/src/lib.rs` already proves container command plans
  preserve `stdin_file`.
- `src/runner/script_command/mod.rs` bridges Rhai `exec::run(...)` to
  `run_container_exec_capture_with_options(...)`.
- `src/tests/runner_tests/run_array_tests/rhai_script_tests.rs` already proves
  file-backed Rhai task re-entry through runner tests.
- `crates/effigy-containers/src/tests/policies.rs` and
  `crates/effigy-containers/src/tests/volumes_reports.rs` already contain
  Underlay-like generated-compose fixtures.

## First Slice

Start with the DecodeLabs mysql seed Rhai proof. It is the smallest slice that
targets the real bug class: a script must not guess whether it is inside or
outside a container, and the SQL file path must be handed to the universal
execution path as structured `stdin_file` data.

## Next Task

Implement card `400`: add DecodeLabs mysql seed Rhai execution proof.
