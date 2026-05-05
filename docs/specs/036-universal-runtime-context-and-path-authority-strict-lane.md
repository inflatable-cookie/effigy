# 036 - Universal Runtime Context And Path Authority Strict Lane

Roadmap: [`g03.030`](../roadmaps/g03/030-universal-runtime-context-and-path-authority.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

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

No active ready card. This lane is complete enough to hand off to `g03.032`.

## Exit Condition

This lane closes when the context contract is live, dispatch uses
`EffigyRuntimeContext`, and remaining context migration work is either complete
or explicitly queued into the next card.

## Next Task

Continue with `037` and card `378`.
