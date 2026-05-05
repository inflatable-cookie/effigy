# 039 - Runtime Container Caller Migration And Cleanup Strict Lane

Roadmap: [`g03.033`](../roadmaps/g03/033-runtime-container-caller-migration-and-cleanup.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Remove duplicated runtime/container glue now that `EffigyRuntimeContext`,
`TaskExecutionRequestBuilder`, and `ContainerManager` exist.

## Hard Boundaries

- do not edit `.github/workflows/`
- do not initiate release commands
- do not change public CLI behavior without an explicit cleanup-break card
- avoid broad formatting churn
- delete compatibility shims only after all internal callers have moved

## Current Ready Card

[`397-decide-runtime-container-cleanup-closeout-boundary.md`](./batch-cards/397-decide-runtime-container-cleanup-closeout-boundary.md)

## Exit Condition

This lane closes when the remaining high-risk runner/container callers use the
new context, manager, and execution request surfaces, and the obsolete wrapper
glue is either removed or documented as a lower-level compatibility boundary.

## Next Task

Complete card `397`.
