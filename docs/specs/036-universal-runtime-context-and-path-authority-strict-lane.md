# 036 - Universal Runtime Context And Path Authority Strict Lane

Roadmap: [`g03.030`](../roadmaps/g03/030-universal-runtime-context-and-path-authority.md)

Status: Active
Owner: Platform
Created: 2026-05-05

## Purpose

Move Effigy from caller-local cwd/root/env probing toward one boot-time runtime
context.

## Hard Boundaries

- do not touch the existing dirty DecodeLabs bootstrap files unless explicitly
  handed over
- do not edit `.github/workflows/`
- do not initiate release commands
- keep public CLI behavior stable in this lane

## Current Ready Card

No active ready card. Card `377` is complete; the next move is either another
`g03.030` caller/context migration card or opening `g03.032` once the execution
request crate work is ready.

## Exit Condition

This lane closes when the context contract is live, dispatch uses
`EffigyRuntimeContext`, and remaining context migration work is either complete
or explicitly queued into the next card.

## Next Task

Choose the next ready card: continue `g03.030` caller/context migration or open
the first `g03.032` execution-builder implementation card.
