# 030 - Universal Runtime Context And Path Authority

Generation: `g03`

Status: Active
Owner: Platform
Created: 2026-05-05
Depends on: [`029-v0-x-release-readiness-audit-and-gate-alignment.md`](./029-v0-x-release-readiness-audit-and-gate-alignment.md)

## Problem

Effigy still recalculates cwd, repo root, host env, and handoff state in too
many caller paths. That makes runtime behavior depend on where a command was
launched from and which caller happened to rediscover context.

## Goal

Create one canonical runtime context and move path/host detection behind it.

## Scope

- add `crates/effigy-context`
- define `EffigyRuntimeContext` and its builder
- capture cwd, command root, repo override, host facts, and container handoff
  state once near CLI entry
- pass the context through runner dispatch
- add `docs/contracts/011-runtime-context-contract.md`
- start replacing direct runner cwd/root recalculation with context-backed
  access

## Non-Goals

- container backend manager extraction
- task request builder extraction
- public CLI changes
- fixing DecodeLabs app bootstrap issues owned by the separate thread

## Exit Condition

This milestone is complete when command dispatch has a canonical context, the
contract is live, and follow-up cards have a bounded migration list for
remaining cwd/root/env probes.

## Next Task

Continue with the next `g03.030` card to migrate command-local cwd/root callers
behind `EffigyRuntimeContext`.
