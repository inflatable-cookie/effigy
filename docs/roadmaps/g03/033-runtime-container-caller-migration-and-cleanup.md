# 033 - Runtime Container Caller Migration And Cleanup

Generation: `g03`

Status: Active
Owner: Platform
Created: 2026-05-05
Started: 2026-05-05
Depends on: [`031-plugin-ready-container-manager-facade.md`](./031-plugin-ready-container-manager-facade.md), [`032-canonical-task-execution-request-and-pipeline.md`](./032-canonical-task-execution-request-and-pipeline.md)

## Goal

Move high-risk runner callers onto the new context, manager, and task request
surfaces, then remove duplicated glue.

## Scope

- migrate standard routed tasks
- migrate managed tasks
- migrate `effigy exec`
- migrate `effigy workspace`
- migrate bootstrap container-backed handoff
- migrate container data operations that shell into services
- shrink the largest mixed-ownership files only after call sites move

## Non-Goals

- broad formatting churn
- unrelated release/docs rewrites

## Next Task

Complete card
[`397-decide-runtime-container-cleanup-closeout-boundary.md`](../../specs/batch-cards/397-decide-runtime-container-cleanup-closeout-boundary.md).
