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

No active ready card. Card `374` is complete; the next card should migrate
command-local cwd/root callers behind `EffigyRuntimeContext`.

## Exit Condition

This lane closes when the context contract is live, dispatch uses
`EffigyRuntimeContext`, and remaining context migration work is either complete
or explicitly queued into the next card.

## Next Task

Create the next `g03.030` migration card for command-local cwd/root callers.
