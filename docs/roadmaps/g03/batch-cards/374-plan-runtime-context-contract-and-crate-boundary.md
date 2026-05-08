# 374 - Plan Runtime Context Contract And Crate Boundary

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: archived
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Open the runtime context lane, define its contract, and land the first
dependency-light context crate.

## Scope

- create `g03.030` through `g03.035` roadmap runway
- create `docs/contracts/011-runtime-context-contract.md`
- add `crates/effigy-context`
- wire CLI dispatch through `EffigyRuntimeContext`
- leave the DecodeLabs dirty files untouched

## Exit Condition

This card is complete when the context contract exists, dispatch has a captured
runtime context, and the next migration card can focus on command-local cwd/root
callers.

## Closeout

The first context slice landed. `EffigyRuntimeContext` captures invocation cwd,
command root, repo override, host facts, and container handoff state. CLI
dispatch now passes the captured context into runner dispatch.

## Next Task

Create the next `g03.030` migration card for command-local cwd/root callers.
