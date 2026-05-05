# 376 - Design Rhai Runtime Context And Execution Helper

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Make the DecodeLabs mysql seed failure a first-class design target for the
runtime/context/execution modularisation work.

## Scope

- define the Rhai read-only runtime context helper backed by
  `EffigyRuntimeContext`
- define the Rhai execution helper backed by `TaskExecutionRequestBuilder`
- specify options for `run_in`, `container`, `service`, `cwd`, `env`, and
  `stdin_file`
- define inside-container behavior so container handoff does not recurse
- inventory first-party Rhai scripts that should move from
  `process::run(...)` or `container::exec(...)` to the execution helper
- include the DecodeLabs mysql seed script as the reference migration

## Exit Condition

This card is complete when the Rhai API contract is clear enough for
`g03.032`, first-party script migrations are listed, and the DecodeLabs seed
proof is represented in the `g03.034` matrix.

## Validation

- docs link check for the updated Rhai/context surfaces
- no code migration required in this design card

## Next Task

Implement this card.
