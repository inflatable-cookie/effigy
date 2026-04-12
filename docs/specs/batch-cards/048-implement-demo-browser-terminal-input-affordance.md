# 048 Implement Demo Browser Terminal Input Affordance

Status: superseded
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

This card is superseded. It proposed browser-side text-entry affordances before
the lane settled the default human interaction model for terminal demos.

## In Scope

- preserved only as traceable history for the superseded browser-first idea

## Out Of Scope

- further execution from this card

## Acceptance Criteria

- the lane uses a fresh ready card aligned to the updated human interaction
  boundary instead of continuing from this superseded browser-first slice

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Stop Conditions

- execution continues from this stale card instead of the recovered lane state

## Next Task

Execute [`049-implement-demo-attached-terminal-run-mode.md`](./049-implement-demo-attached-terminal-run-mode.md)
to make direct attached terminal sessions the default human path for demos that
need interactive terminal IO, while keeping `demo input` as secondary
automation/client infrastructure.
