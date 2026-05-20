# 1017 - Reduce Script Command Owner Sprawl

Roadmap: [`../067-script-command-boundary-reduction.md`](../067-script-command-boundary-reduction.md)
Strict lane: [`../../../specs/095-residual-maintainability-follow-through-strict-lane.md`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-19

## Purpose

Reduce `script_command/mod.rs` by pulling out the clearest pure or mostly-pure
owners first.

## Work

- inspect `script_command/mod.rs` for planning, resolution, staging, and
  execution seams
- extract only the obvious local owners
- preserve command behavior and error text
- run focused runner validation before moving on

## Guardrails

- no runner-wide architecture rewrite
- no shell/process glue migration into domain crates
- no hidden behavior changes inside file movement

## Acceptance

- `script_command/mod.rs` is materially clearer or smaller
- focused runner tests pass

## Next Task

Execute `1018`.
