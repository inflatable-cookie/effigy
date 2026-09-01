# 112 Rhai Profile-Independent Limits Strict Lane

Status: Ready
Created: 2026-09-01
Roadmap: [`g08.039`](../roadmaps/g08/039-rhai-profile-independent-limits-papercut.md)
Card: [`1094`](../roadmaps/g08/batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md)
Guide: [`061`](../guides/061-rhai-script-steps-guide.md)

## Outcome

Effigy scripts have the same finite parser expression envelope in debug and
release builds.

## Problem

The Rhai dependency changes default parser limits under `debug_assertions`.
Effigy currently constructs a raw engine and inherits function depth `16` in
debug and `32` in release, so a script can pass the installed release binary and
fail the documented source-build fallback.

## Decisions

- Effigy explicitly sets global expression depth `64` and function expression
  depth `32`, matching the current release defaults.
- These limits are host-owned constants applied through one shared configured
  engine seam.
- Finite bounds remain mandatory; this lane does not set either value to zero.
- Other Rhai limits and every registered host capability remain unchanged.

## Scope

- `effigy-rhai` engine construction
- exact-limit and adversarial parser/runtime proof
- checked-in first-party script regression proof
- guide, changelog, papercut, evidence, and strict-lane closeout

## Acceptance

- every production Rhai route uses one explicit `64` / `32` configuration
- a debug build accepts a function expression above `16` and within `32`
- a function expression above `32` remains rejected
- debug and release focused suites assert the same limit values
- current first-party scripts and benchmark run successfully
- full validation passes without new public configuration or behavior outside
  the parsing envelope

## Non-Goals

- configurable Rhai limits
- unlimited depth or relaxed call-stack/operation/data limits
- provider, storage, S3, extension, or catalog-pack movement
- graph timeout/progress work
- release execution

## Stop Conditions

Return to the orchestrator if implementation needs a public threshold choice,
changes a non-expression Rhai limit, bypasses a finite upper bound, moves a host
API, changes manifest/CLI grammar, or exposes a broader dependency/runtime
compatibility issue.

## Next Task

Execute ready card
[`1094`](../roadmaps/g08/batch-cards/1094-fix-rhai-profile-dependent-expression-limits.md).
