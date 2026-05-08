# 412 - Widen Container Runtime Contract For Manager And Context

Lane: [`041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md`](../041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Update `005-container-runtime-contract.md` so it references the shipped runtime
context and container manager ownership.

## Scope

- name `EffigyRuntimeContext` as the source for handoff and path facts
- name `ContainerManager` as the runner-facing operation owner
- preserve the existing local runtime guarantee language
- no implementation changes

## Exit Condition

This card is complete when `005` no longer implies that runtime prep or
container-backed execution can re-probe context or branch on Docker/Colima
locally in runner code.

## Closeout

Updated `005-container-runtime-contract.md` to name
`EffigyRuntimeContext`, `ContainerManager`, and
`TaskExecutionRequestBuilder` as the relevant context, manager, and request
authorities for container-backed local execution.

## Next Task

Decide cleanup-break and changelog boundary.
