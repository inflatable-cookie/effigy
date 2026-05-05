# 405 - Add Execution Surface Plan Parity Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Ready
Owner: Platform
Created: 2026-05-05

## Goal

Prove direct task, bootstrap task, and Rhai task entrypoints produce equivalent
resolved execution plans for the same selector/context inputs.

## Scope

- add or tighten focused `effigy-execution` proof coverage
- compare direct CLI, bootstrap, and Rhai surfaces for the same task selector,
  runtime context, runtime policy, and env/stdin inputs
- assert route and environment parity while preserving each surface label
- no public CLI behavior changes

## Exit Condition

This card is complete when execution-plan parity fails if direct, bootstrap, or
Rhai task request construction diverges for equivalent inputs.

## Next Task

Add the execution-surface plan parity proof.
