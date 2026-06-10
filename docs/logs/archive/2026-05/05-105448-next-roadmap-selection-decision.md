# 2026-05-05 - Next Roadmap Selection Decision

## Summary

Completed card `372` and selected `g03.029` as the next live roadmap.

## Decision

The next lane is a `v0.x` release-readiness audit and gate-alignment pass.

## Evidence

- `g03.019` says the `v0.x` release contract remains the live authority until
  additional features and tidy-up ship.
- `g03.020` says distribution channels are closed and should be revisited only
  if a new install channel is explicitly requested.
- `g03.027` closed prompt guardrails, adding meaningful user-facing behavior
  that should be included in the next readiness audit.
- the backlog front door has no promoted item.

## Boundary

This is not release execution. The next lane may inspect gates, changelog
coverage, docs, and status surfaces, but release prepare/execute/tagging still
requires explicit human instruction.

## Current State

- `g03.028`: complete
- strict lane `034`: complete
- card `372`: complete
- active roadmap: `g03.029`
- active strict lane: `035`
- current ready card: `373`

## Validation

- `git diff --check`
