# 467 - Select Next Runtime Activation Caller Migration

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next caller migration for `g04.003` after exec, DB seed, deferral,
standard, managed, and workspace paths are activation-plan-backed.

## Scope

- review remaining activation-sensitive callers
- choose one next implementation card:
  - bootstrap container-backed handoff
  - Rhai container-sensitive execution
  - activation report plumbing
  - close `g04.003` if the remaining work belongs in later milestones
- update the lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next ready card is selected and scoped.

## Closeout

Decision:

- migrate Rhai container-sensitive execution next

Rationale:

- `exec::run(..., #{ run_in: "container" })` already builds a
  `TaskExecutionRequest`, but the runner callback still drops into
  `run_container_exec_capture_with_options` without activation-plan ownership
- this is the bug class that originally motivated the runtime context and task
  execution builder work
- bootstrap workspace handoff is now activation-plan-backed through the
  workspace session path, while Rhai still has a direct container exec callback
  seam to close

## Validation

- `git diff --check`

## Next Task

Route Rhai container exec callbacks through runtime activation planning.
