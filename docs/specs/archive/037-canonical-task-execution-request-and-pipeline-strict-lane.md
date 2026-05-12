# 037 - Canonical Task Execution Request And Pipeline Strict Lane

Roadmap: [`g03.032`](../roadmaps/g03/032-canonical-task-execution-request-and-pipeline.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Purpose

Move task and command execution intent into one reusable request model before
migrating direct tasks, embedded callers, and Rhai `exec::run(...)`.

## Hard Boundaries

- do not change public CLI behavior in the first crate slice
- do not edit `.github/workflows/`
- do not initiate release commands
- do not rework container backend management here; that belongs to `g03.031`
- keep DecodeLabs/Underlay app-specific fixes out of this lane except as
  fixtures or first-party Rhai migrations

## Current Ready Card

No active ready card. Lane `037` is complete.

## Exit Condition

This lane closes when direct CLI, embedded task callers, and Rhai command
execution can build execution through `TaskExecutionRequestBuilder`, with
runtime/context and host/container intent expressed once.

## Next Task

Continue with `g03.031` container manager facade or choose the next queued
roadmap deliberately.
