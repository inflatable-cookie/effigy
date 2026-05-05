# 032 - Canonical Task Execution Request And Pipeline

Generation: `g03`

Status: Queued
Owner: Platform
Created: 2026-05-05
Depends on: [`030-universal-runtime-context-and-path-authority.md`](./030-universal-runtime-context-and-path-authority.md)

## Goal

Make task execution explicit and reusable from direct CLI, deferral, bootstrap,
Rhai, run-array, demo, and managed flows.

## Scope

- add `crates/effigy-execution`
- define `TaskExecutionRequestBuilder`
- move task preflight discovery into one request path
- make runner execution consume a resolved request/plan
- replace ad hoc task invocation construction in embedded callers

## Non-Goals

- public CLI changes by default
- container backend extraction beyond what `g03.031` owns

## Next Task

Wait for `g03.030` context contract to stabilize before opening the execution
request lane.
